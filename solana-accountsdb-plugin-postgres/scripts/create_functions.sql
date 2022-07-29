
-----------------------------------------------------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION find_slot_on_longest_branch(transaction_slots BIGINT[])
RETURNS BIGINT
AS $find_slot_on_longest_branch$
DECLARE
    current_slot BIGINT := NULL;
    current_slot_status VARCHAR := NULL;
    num_in_txn_slots INT := 0;
BEGIN
    -- start from topmost slot
    SELECT s.slot
    INTO current_slot
    FROM slot AS s
    ORDER BY s.slot DESC LIMIT 1;
  
    LOOP
        -- get status of current slot
        SELECT s.status
        INTO current_slot_status
        FROM slot AS s
        WHERE s.slot = current_slot;
    
        -- already on rooted slot - stop iteration
        IF current_slot_status = 'rooted' THEN
            RETURN NULL;
        END IF;
    
        -- does current slot contain transaction ?
        SELECT COUNT(*)
        INTO num_in_txn_slots
        FROM unnest(transaction_slots) AS slot
        WHERE slot = current_slot;
    
        -- if yes - it means we found slot with txn
        -- on the longest branch - return it
        IF num_in_txn_slots <> 0 THEN
            RETURN current_slot;
        END IF;
    
        -- If no - go further into the past - select parent slot
        SELECT s.parent
        INTO current_slot
        FROM slot AS s
        WHERE s.slot = current_slot;
    END LOOP;
END;
$find_slot_on_longest_branch$ LANGUAGE plpgsql;

-----------------------------------------------------------------------------------------------------------------------
-- Returns pre-accounts data for given transaction on a given slot
CREATE OR REPLACE FUNCTION get_pre_accounts_one_slot(
    current_slot BIGINT,
    max_write_version BIGINT,
    transaction_accounts BYTEA[]
)

RETURNS TABLE (
    pubkey BYTEA,
    slot BIGINT,
    write_version BIGINT,
    signature BYTEA,
    data BYTEA
)

AS $get_pre_accounts_one_slot$

BEGIN
    RETURN QUERY
        SELECT DISTINCT ON (acc.pubkey)
            acc.pubkey,
            acc.slot,
            acc.write_version,
            acc.txn_signature,
            acc.data
        FROM account_audit AS acc
        WHERE
            acc.slot = current_slot
            AND acc.write_version < max_write_version
            AND acc.pubkey IN (SELECT * FROM unnest(transaction_accounts))
        ORDER BY
            acc.pubkey,
            acc.write_version DESC;
END;
$get_pre_accounts_one_slot$ LANGUAGE plpgsql;

-----------------------------------------------------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION get_pre_accounts_branch(
    start_slot BIGINT,
    max_write_version BIGINT,
    transaction_accounts BYTEA[]
)
  
RETURNS TABLE (
    pubkey BYTEA,
    slot BIGINT,
    write_version BIGINT,
    signature BYTEA,
    data BYTEA
)

AS $get_pre_accounts_branch$

DECLARE
    branch_slots BIGINT[];

BEGIN   
    WITH RECURSIVE parents AS (
        SELECT
            first.slot,
            first.parent,
            first.status
        FROM slot AS first
        WHERE first.slot = start_slot
        UNION
            SELECT
                next.slot,
                next.parent,
                next.status
            FROM slot AS next
            INNER JOIN parents p ON p.parent = next.slot
    )
    SELECT array_agg(prnts.slot)
    INTO branch_slots
    FROM parents AS prnts;
   
    RETURN QUERY
        SELECT DISTINCT ON (slot_results.pubkey)
            slot_results.pubkey,
            slot_results.slot,
            slot_results.write_version,
            slot_results.signature,
            slot_results.data
        FROM
            unnest(branch_slots) AS current_slot,
            get_pre_accounts_one_slot(
                current_slot, 
                max_write_version, 
                transaction_accounts
            ) AS slot_results
        ORDER BY
            slot_results.pubkey,
            slot_results.slot DESC,
            slot_results.write_version DESC;
END;
$get_pre_accounts_branch$ LANGUAGE plpgsql;

-----------------------------------------------------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION get_pre_accounts_root(
    start_slot BIGINT,
    max_write_version BIGINT,
    transaction_accounts BYTEA[]
)

RETURNS TABLE (
    pubkey BYTEA,
    slot BIGINT,
    write_version BIGINT,
    signature BYTEA,
    data BYTEA
)

AS $get_pre_accounts_root$

BEGIN
    RETURN QUERY
        SELECT DISTINCT ON (acc.pubkey)
            acc.pubkey,
            acc.slot,
            acc.write_version,
            acc.txn_signature,
            acc.data
        FROM account_audit AS acc
        WHERE
            acc.slot <= start_slot
            AND acc.write_version < max_write_version
            AND acc.pubkey IN (SELECT * FROM unnest(transaction_accounts))
        ORDER BY
            acc.pubkey DESC,
            acc.write_version DESC;
END;
$get_pre_accounts_root$ LANGUAGE plpgsql;

-----------------------------------------------------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION get_pre_accounts(in_txn_signature BYTEA) 
RETURNS TABLE (
    pubkey BYTEA,
    slot BIGINT,
    write_version BIGINT,
    signature BYTEA,
    data BYTEA
)

AS $get_pre_accounts$

DECLARE
    current_slot BIGINT;
    max_write_version BIGINT := NULL;
    transaction_accounts BYTEA[];
    transaction_slots BIGINT[];
    first_rooted_slot BIGINT;
   
BEGIN                    
    -- Query minimum write version of account update
    SELECT MIN(acc.write_version)
    INTO max_write_version
    FROM account_audit AS acc
    WHERE position(in_txn_signature in acc.txn_signature) > 0;
  
    -- Query all accounts required by given transaction
    SELECT array_agg(ta.pubkey)
    INTO transaction_accounts
    FROM transaction_account AS ta
    WHERE position(in_txn_signature IN ta.signature) > 0;
  
    -- find all occurencies of transaction in slots
    SELECT array_agg(txn.slot)
    INTO transaction_slots
    FROM transaction AS txn
    WHERE position(in_txn_signature in txn.signature) > 0;
  
    -- Query first rooted slot
    SELECT sl.slot
    INTO first_rooted_slot
    FROM slot AS sl
    WHERE sl.status = 'rooted'
    ORDER BY sl.slot DESC
    LIMIT 1;
  
    -- try to find slot that was rooted with given transaction
    SELECT txn_slot INTO current_slot
    FROM unnest(transaction_slots) AS txn_slot
    INNER JOIN slot AS s
    ON txn_slot = s.slot
    WHERE s.status = 'rooted'
    LIMIT 1;
  
    IF current_slot IS NULL THEN
        -- No rooted slot found. It means transaction exist on some not finalized branch.
        -- Try to find it on the longest one (search from topmost slot down to first rooted slot)
        SELECT find_slot_on_longest_branch(transaction_slots) INTO current_slot;
        IF current_slot IS NULL THEN
            -- Transaction not found on the longest branch - it exist somewhere on minor forks.
            -- Return empty list of accounts
            RETURN;
        ELSE
            -- Transaction found on the longest branch. 
            RETURN QUERY
                WITH results AS (
                    -- Start searching recent states of accounts in this branch
                    -- down to first rooted slot 
                    -- (this search algorithm iterates over parent slots and is slow).
                    SELECT * FROM get_pre_accounts_branch(
                        current_slot,
                        max_write_version,
                        transaction_accounts
                    )
                    UNION
                    -- Then apply fast search algorithm over rooted slots 
                    -- to obtain the rest of pre-accounts  
                    SELECT * FROM get_pre_accounts_root(
                        first_rooted_slot,
                        max_write_version,
                        transaction_accounts
                    )
                )
                SELECT DISTINCT ON (res.pubkey)
                    res.pubkey,
                    res.slot,
                    res.write_version,
                    res.signature,
                    res.data
                FROM results AS res
                ORDER BY
                    res.pubkey,
                    res.slot DESC,
                    res.write_version DESC;
        END IF;
    ELSE
        -- Transaction found on the rooted slot.
        RETURN QUERY
            SELECT * FROM get_pre_accounts_root(
                current_slot,
                max_write_version,
                transaction_accounts
            );
    END IF;
END;
$get_pre_accounts$ LANGUAGE plpgsql;

-----------------------------------------------------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION get_accounts_at_slot(
    in_pubkeys BYTEA[],
    in_slot BIGINT
)

RETURNS TABLE (
    pubkey BYTEA,
	owner BYTEA,
	lamports BIGINT,
	executable BOOL,
	rent_epoch BIGINT,
	data BYTEA,
    slot BIGINT,
    write_version BIGINT,
    signature BYTEA
)

AS $get_accounts_at_slot$

BEGIN
    RETURN QUERY
        SELECT
            acc.pubkey,
            acc.owner,
            acc.lamports,
            acc.executable,
            acc.rent_epoch,
            acc.data,
            acc.slot,
            acc.write_version,
            acc.txn_signature
        FROM account_audit AS acc
        WHERE
            acc.slot <= in_slot
            AND acc.pubkey = ANY(in_pubkeys)
        ORDER BY
            acc.slot DESC, acc.write_version DESC
        LIMIT 1;
END;
$get_accounts_at_slot$ LANGUAGE plpgsql;

