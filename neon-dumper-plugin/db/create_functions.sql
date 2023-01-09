
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
CREATE OR REPLACE FUNCTION find_txn_slot_on_longest_branch(in_txn_signature BYTEA)
RETURNS BIGINT
AS $find_txn_slot_on_longest_branch$
DECLARE
    transaction_slots BIGINT[];
    current_slot BIGINT := NULL;
    current_slot_status VARCHAR := NULL;
    num_in_txn_slots INT := 0;
BEGIN
    -- find all occurencies of transaction in slots
    SELECT array_agg(txn.slot)
    INTO transaction_slots
    FROM transaction AS txn
    WHERE position(in_txn_signature in txn.signature) > 0;

    -- try to find slot that was rooted with given transaction
    SELECT txn_slot INTO current_slot
    FROM unnest(transaction_slots) AS txn_slot
         INNER JOIN slot AS s
                    ON txn_slot = s.slot
    WHERE s.status = 'rooted'
    LIMIT 1;

    IF current_slot IS NOT NULL THEN
        RETURN current_slot;
    END IF;

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
$find_txn_slot_on_longest_branch$ LANGUAGE plpgsql;

-----------------------------------------------------------------------------------------------------------------------
-- Returns pre-accounts data for given transaction on a given slot
CREATE OR REPLACE FUNCTION get_pre_accounts_one_slot(
    current_slot BIGINT,
    max_write_version BIGINT,
    transaction_accounts BYTEA[]
)

RETURNS TABLE (
    lamports BIGINT,
    data BYTEA,
    owner BYTEA,
    executable BOOL,
    rent_epoch BIGINT,
    pubkey BYTEA,
    slot BIGINT,
    write_version BIGINT,
    signature BYTEA
)

AS $get_pre_accounts_one_slot$

BEGIN
    RETURN QUERY
        SELECT DISTINCT ON (acc.pubkey)
            acc.lamports,
            acc.data,
            acc.owner,
            acc.executable,
            acc.rent_epoch,
            acc.pubkey,
            acc.slot,
            acc.write_version,
            acc.txn_signature
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
    lamports BIGINT,
    data BYTEA,
    owner BYTEA,
    executable BOOL,
    rent_epoch BIGINT,
    pubkey BYTEA,
    slot BIGINT,
    write_version BIGINT,
    signature BYTEA
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
        WHERE first.slot = start_slot and first.status <> 'rooted'
        UNION
            SELECT
                next.slot,
                next.parent,
                next.status
            FROM slot AS next
            INNER JOIN parents p ON p.parent = next.slot
            WHERE next.status <> 'rooted'
    )
    SELECT array_agg(prnts.slot)
    INTO branch_slots
    FROM parents AS prnts;
   
    RETURN QUERY
        SELECT DISTINCT ON (slot_results.pubkey)
            slot_results.lamports,
            slot_results.data,
            slot_results.owner,
            slot_results.executable,
            slot_results.rent_epoch,
            slot_results.pubkey,
            slot_results.slot,
            slot_results.write_version,
            slot_results.signature
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
    lamports BIGINT,
    data BYTEA,
    owner BYTEA,
    executable BOOL,
    rent_epoch BIGINT,
    pubkey BYTEA,
    slot BIGINT,
    write_version BIGINT,
    signature BYTEA
)

AS $get_pre_accounts_root$

BEGIN
    RETURN QUERY
        SELECT DISTINCT ON (acc.pubkey)
            acc.lamports,
            acc.data,
            acc.owner,
            acc.executable,
            acc.rent_epoch,
            acc.pubkey,
            acc.slot,
            acc.write_version,
            acc.txn_signature
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
CREATE OR REPLACE FUNCTION get_pre_accounts(
    in_txn_signature BYTEA,
    transaction_accounts BYTEA[]
)
RETURNS TABLE (
    lamports BIGINT,
    data BYTEA,
    owner BYTEA,
    executable BOOL,
    rent_epoch BIGINT,
    pubkey BYTEA,
    slot BIGINT,
    write_version BIGINT,
    signature BYTEA
)

AS $get_pre_accounts$

DECLARE
    current_slot BIGINT;
    max_write_version BIGINT := NULL;
    transaction_slots BIGINT[];
    first_rooted_slot BIGINT;
   
BEGIN                    
    -- Query minimum write version of account update
    SELECT MIN(acc.write_version)
    INTO max_write_version
    FROM account_audit AS acc
    WHERE position(in_txn_signature in acc.txn_signature) > 0;
  
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
                    res.lamports,
                    res.data,
                    res.owner,
                    res.executable,
                    res.rent_epoch,
                    res.pubkey,
                    res.slot,
                    res.write_version,
                    res.signature
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
CREATE OR REPLACE FUNCTION get_account_at_slot(
    in_pubkey BYTEA,
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

AS $get_account_at_slot$

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
            AND acc.pubkey = in_pubkey
        ORDER BY
            acc.slot DESC, acc.write_version DESC
        LIMIT 1;
END;
$get_account_at_slot$ LANGUAGE plpgsql;

-----------------------------------------------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION get_recent_blockhashes(start_slot BIGINT, num_hashes INT)

RETURNS TABLE (
	slot BIGINT,
    blockhash VARCHAR(44)
)

AS $get_recent_blockhashes$

DECLARE
    branch_slots BIGINT[];

BEGIN
    WITH RECURSIVE parents(slot, parent, status, depth) AS (
        SELECT
            first.slot,
            first.parent,
            first.status,
            1
        FROM slot AS first
        WHERE first.slot = start_slot
        UNION
            SELECT
                next.slot,
                next.parent,
                next.status,
                depth + 1
            FROM slot AS next
            INNER JOIN parents p ON p.parent = next.slot
            WHERE depth < num_hashes
    )
    SELECT array_agg(prnts.slot)
    INTO branch_slots
    FROM parents AS prnts;

    RETURN QUERY
        SELECT b.slot, b.blockhash
        FROM block AS b
        WHERE b.slot = ANY(branch_slots);
END;
$get_recent_blockhashes$ LANGUAGE plpgsql;

-----------------------------------------------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION numeric_to_bytea(_n NUMERIC) RETURNS BYTEA AS $numeric_to_bytea$
DECLARE
    _b BYTEA := '\x';
    _v INTEGER;
BEGIN
    WHILE _n > 0 LOOP
            _v := _n % 256;
            _b := SET_BYTE(('\x00' || _b),0,_v);
            _n := (_n-_v)/256;
        END LOOP;
    RETURN _b;
END;
$numeric_to_bytea$ LANGUAGE PLPGSQL IMMUTABLE STRICT;

-----------------------------------------------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION base58_to_bytea(str VARCHAR(255)) RETURNS BYTEA AS $base58_to_bytea$
DECLARE
    alphabet VARCHAR(255);
    c CHAR(1);
    p INT;
    v NUMERIC(155);
BEGIN
    alphabet := '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
    v := 0;
    FOR i IN 1..char_length(str) LOOP
            c := substring(str FROM i FOR 1);
            -- This is probably wildly inefficient, but we're just using this function for diagnostics...
            p := position(c IN alphabet);
            IF p = 0 THEN
                RAISE 'Illegal base58 character ''%'' in ''%''', c, str;
            END IF;
            v := (v * 58) + (p - 1);
        END LOOP;

    RETURN numeric_to_bytea(v);
END;
$base58_to_bytea$ LANGUAGE PLPGSQL;