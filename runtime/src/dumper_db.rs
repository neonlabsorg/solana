use im::HashSet;
use {
    log::*,
    itertools::Itertools,
    openssl::ssl::{SslConnector, SslFiletype, SslMethod},
    postgres::{Client, NoTls, Row, Statement},
    postgres_openssl::MakeTlsConnector,
    postgres_types::{FromSql, ToSql},
    solana_sdk::{
        account::{ AccountSharedData, WritableAccount },
        clock::Slot, pubkey::Pubkey,
        instruction::CompiledInstruction,
        message::{
            Message as LegacyMessage, v0::Message as V0Message, MessageHeader, VersionedMessage,
            v0::MessageAddressTableLookup, SanitizedMessage,
        },
        signature::Signature,
        transaction::{ SanitizedTransaction, SanitizedVersionedTransaction, VersionedTransaction, AddressLoader },
        hash::Hash,
    },
    std::{ collections::BTreeMap, sync::{ Mutex, MutexGuard }},
    thiserror::Error,
};

#[derive(Clone, Debug, FromSql, ToSql)]
#[postgres(name = "CompiledInstruction")]
pub struct DbCompiledInstruction {
    pub program_id_index: i16,
    pub accounts: Vec<i16>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, FromSql, ToSql)]
#[postgres(name = "TransactionMessageHeader")]
pub struct DbTransactionMessageHeader {
    pub num_required_signatures: i16,
    pub num_readonly_signed_accounts: i16,
    pub num_readonly_unsigned_accounts: i16,
}

#[derive(Clone, Debug, FromSql, ToSql)]
#[postgres(name = "TransactionMessage")]
pub struct DbTransactionMessage {
    pub header: DbTransactionMessageHeader,
    pub account_keys: Vec<Vec<u8>>,
    pub recent_blockhash: Vec<u8>,
    pub instructions: Vec<DbCompiledInstruction>,
}

#[derive(Clone, Debug, FromSql, ToSql)]
#[postgres(name = "TransactionMessageAddressTableLookup")]
pub struct DbTransactionMessageAddressTableLookup {
    pub account_key: Vec<u8>,
    pub writable_indexes: Vec<i16>,
    pub readonly_indexes: Vec<i16>,
}

#[derive(Clone, Debug, FromSql, ToSql)]
#[postgres(name = "TransactionMessageV0")]
pub struct DbTransactionMessageV0 {
    pub header: DbTransactionMessageHeader,
    pub account_keys: Vec<Vec<u8>>,
    pub recent_blockhash: Vec<u8>,
    pub instructions: Vec<DbCompiledInstruction>,
    pub address_table_lookups: Vec<DbTransactionMessageAddressTableLookup>,
}

pub struct DumperDb {
    client: Mutex<Client>,
    get_accounts_at_slot_statement: Statement,
    get_block_statement: Statement,
    get_transaction_statement: Statement,
    get_pre_accounts_statement: Statement,
    get_recent_blockhashes_statement: Statement,
    find_txn_slot_on_longest_branch_statement: Statement,
}

impl std::fmt::Debug for DumperDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

pub struct DumperDbConfig {
    pub port: Option<u16>,
    pub connection_str: Option<String>,
    pub host: Option<String>,
    pub user: Option<String>,
    pub use_ssl: Option<bool>,
    pub server_ca: Option<String>,
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
}

const DEFAULT_POSTGRES_PORT: u16 = 5432;

#[derive(Error, Debug)]
pub enum DumperDbError {
    #[error("DB lock error: {msg}")]
    DbLockError{ msg: String },

    #[error("Error loading account {pubkey} (slot {slot}): {err}")]
    LoadAccError{ pubkey: Pubkey, slot: Slot, err: postgres::Error },

    #[error("More than one occurence of account {pubkey} found in slot {slot}")]
    TooManyAccs{ pubkey: Pubkey, slot: Slot },

    #[error("Failed to read field {name}")]
    FailedReadField{ name: String },

    #[error("Failed get pre-accounts {signature}: {err}")]
    FailedGetPreAccounts{ signature: Signature, err: postgres::Error },

    #[error("Failed get transaction {signature} at slot {slot}: {err}")]
    GetTransactionError{ signature: Signature, slot: Slot, err: postgres::Error },

    #[error("Transaction {signature} not found at slot {slot}")]
    TransactionNotFound{ signature: Signature, slot: Slot },

    #[error("Failed read message for transaction {signature} slot {slot}")]
    FailedReadMessage{ signature: Signature, slot: Slot },

    #[error("Failed create VersionedTransaction {signature}: {err}")]
    FailedCreateVersionedTransaction{ signature: Signature, err: solana_sdk::sanitize::SanitizeError },

    #[error("Failed create SanitizedTransaction {signature}: {err}")]
    FailedCreateSanitizedTransaction{ signature: Signature, err: solana_sdk::transaction::TransactionError },

    #[error("Failed read recent blockhashes start_slot = {start_slot} num_hashes = {num_hashes}: {err}")]
    FailedReadRecentBlockhashes{ start_slot: Slot, num_hashes: u32, err: postgres::Error },

    #[error("Failed find transaction {signature} slot: {err}")]
    FailedQueryTransactionSlot{ signature: Signature, err: postgres::Error },

    #[error("Transaction {signature} slot not found")]
    TransactionSlotNotFound{ signature: Signature },

    #[error("Failed query block slot={slot}: {err}")]
    FailedQueryBlock{ slot: Slot, err: postgres::Error },

    #[error("Only one block expected")]
    NotOneBlock,

    #[error("Custom error: ({msg})")]
    Custom{ msg: String },
}

pub struct Block {
    pub slot: u64,
    pub blockhash: String,
    pub block_time: Option<i64>,
    pub block_height: Option<i64>,
}

impl DumperDb {
    pub fn connect_to_db(config: &DumperDbConfig) -> Result<Client, DumperDbError> {
        let port = config.port.unwrap_or(DEFAULT_POSTGRES_PORT);

        let connection_str = if let Some(connection_str) = &config.connection_str {
            connection_str.clone()
        } else {
            if config.host.is_none() || config.user.is_none() {
                let msg = format!(
                    "\"connection_str\": {:?}, or \"host\": {:?} \"user\": {:?} must be specified",
                    config.connection_str, config.host, config.user
                );
                return Err(DumperDbError::Custom{ msg });
            }
            format!(
                "host={} user={} port={}",
                config.host.as_ref().unwrap(),
                config.user.as_ref().unwrap(),
                port
            )
        };

        let result = if let Some(true) = config.use_ssl {
            if config.server_ca.is_none() {
                let msg = "\"server_ca\" must be specified when \"use_ssl\" is set".to_string();
                return Err(DumperDbError::Custom{ msg });
            }
            if config.client_cert.is_none() {
                let msg = "\"client_cert\" must be specified when \"use_ssl\" is set".to_string();
                return Err(DumperDbError::Custom{ msg });
            }
            if config.client_key.is_none() {
                let msg = "\"client_key\" must be specified when \"use_ssl\" is set".to_string();
                return Err(DumperDbError::Custom{ msg });
            }
            let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
            if let Err(err) = builder.set_ca_file(config.server_ca.as_ref().unwrap()) {
                let msg = format!(
                    "Failed to set the server certificate specified by \"server_ca\": {}. Error: ({})",
                    config.server_ca.as_ref().unwrap(), err);
                return Err(DumperDbError::Custom{ msg });
            }
            if let Err(err) =
            builder.set_certificate_file(config.client_cert.as_ref().unwrap(), SslFiletype::PEM)
            {
                let msg = format!(
                    "Failed to set the client certificate specified by \"client_cert\": {}. Error: ({})",
                    config.client_cert.as_ref().unwrap(), err);
                return Err(DumperDbError::Custom{ msg });
            }
            if let Err(err) =
            builder.set_private_key_file(config.client_key.as_ref().unwrap(), SslFiletype::PEM)
            {
                let msg = format!(
                    "Failed to set the client key specified by \"client_key\": {}. Error: ({})",
                    config.client_key.as_ref().unwrap(),
                    err
                );
                return Err(DumperDbError::Custom{ msg });
            }

            let mut connector = MakeTlsConnector::new(builder.build());
            connector.set_callback(|connect_config, _domain| {
                connect_config.set_verify_hostname(false);
                Ok(())
            });
            Client::connect(&connection_str, connector)
        } else {
            Client::connect(&connection_str, NoTls)
        };

        match result {
            Err(err) => {
                let msg = format!(
                    "Error in connecting to the PostgreSQL database: {:?} connection_str: {:?}",
                    err, connection_str
                );
                error!("{}", msg);
                Err(DumperDbError::Custom{ msg })
            }
            Ok(client) => Ok(client),
        }
    }

    fn create_get_accounts_at_slot_statement(client: &mut Client) -> Result<Statement, DumperDbError> {
        let stmt = "SELECT lamports, data, owner, executable, rent_epoch FROM get_accounts_at_slot($1, $2)";
        let stmt = client.prepare(stmt);
        stmt.map_err(|err| {
            let msg = format!("Failed to prepare get_account_at_slot statement: {}", err);
            DumperDbError::Custom { msg }
        })
    }

    fn create_get_block_statement(client: &mut Client) -> Result<Statement, DumperDbError> {
        let stmt = "SELECT * FROM block WHERE slot = $1";
        let stmt = client.prepare(stmt);
        stmt.map_err(|err| {
            let msg = format!("Failed to prepare get_block statement: {}", err);
            DumperDbError::Custom { msg }
        })
    }

    fn create_get_transaction_statement(client: &mut Client) -> Result<Statement, DumperDbError> {
        let stmt = "SELECT * FROM transaction WHERE slot = $1 and position($2 in signature) > 0;";
        let stmt = client.prepare(stmt);
        stmt.map_err(|err| {
            let msg = format!("Failed to prepare get_transaction statement: {}", err);
            DumperDbError::Custom { msg }
        })
    }

    fn create_get_pre_accounts_statement(client: &mut Client) -> Result<Statement, DumperDbError> {
        let stmt = "SELECT lamports, data, owner, executable, rent_epoch, pubkey FROM get_pre_accounts($1, $2);";
        let stmt = client.prepare(stmt);
        stmt.map_err(|err| {
            let msg = format!("Failed to prepare get_pre_accounts statement: {}", err);
            DumperDbError::Custom { msg }
        })
    }

    fn create_get_recent_blockhashes_statement(client: &mut Client) -> Result<Statement, DumperDbError> {
        let stmt = "SELECT * FROM get_recent_blockhashes($1, $2);";
        let stmt = client.prepare(stmt);
        stmt.map_err(|err| {
            let msg = format!("Failed to prepare get_recent_blockhashes statement: {}", err);
            DumperDbError::Custom { msg }
        })
    }

    fn create_find_txn_slot_on_longest_branch_statement(client: &mut Client) -> Result<Statement, DumperDbError> {
        let stmt = "SELECT * FROM find_txn_slot_on_longest_branch($1);";
        let stmt = client.prepare(stmt);
        stmt.map_err(|err| {
            let msg = format!("Failed to prepare find_txn_slot_on_longest_branch_statement statement: {}", err);
            DumperDbError::Custom { msg }
        })
    }

    pub fn new(config: &DumperDbConfig) -> Result<Self, DumperDbError> {
        info!("Creating Postgres Client...");
        let mut client = Self::connect_to_db(config)?;

        let get_accounts_at_slot_statement = Self::create_get_accounts_at_slot_statement(&mut client)?;
        let get_block_statement = Self::create_get_block_statement(&mut client)?;
        let get_transaction_statement = Self::create_get_transaction_statement(&mut client)?;
        let get_pre_accounts_statement = Self::create_get_pre_accounts_statement(&mut client)?;
        let get_recent_blockhashes_statement = Self::create_get_recent_blockhashes_statement(&mut client)?;
        let find_txn_slot_on_longest_branch_statement = Self::create_find_txn_slot_on_longest_branch_statement(&mut client)?;

        info!("Created Postgres Client.");
        Ok(Self {
            client: Mutex::new(client),
            get_accounts_at_slot_statement,
            get_block_statement,
            get_transaction_statement,
            get_pre_accounts_statement,
            get_recent_blockhashes_statement,
            find_txn_slot_on_longest_branch_statement,
        })
    }

    fn read_account(row: &Row) -> Result<AccountSharedData, DumperDbError> {
        let lamports: i64 = row.try_get(0).map_err(|err| DumperDbError::FailedReadField {
            name: "lamports".to_string()
        })?;

        let data = row.try_get(1).map_err(|err| DumperDbError::FailedReadField {
            name: "data".to_string()
        })?;

        let owner: Vec<u8>= row.try_get(2).map_err(|err| DumperDbError::FailedReadField {
            name: "owner".to_string()
        })?;

        let executable = row.try_get(3).map_err(|err| DumperDbError::FailedReadField {
            name: "executable".to_string()
        })?;

        let rent_epoch: i64 = row.try_get(4).map_err(|err| DumperDbError::FailedReadField {
            name: "rent_epoch".to_string()
        })?;

        let account = AccountSharedData::create(
            lamports as u64,
            data,
            Pubkey::new(&owner),
            executable,
            rent_epoch as u64
        );

        Ok(account)
    }

    fn lock_client(&self) -> Result<MutexGuard<Client>, DumperDbError> {
        self.client.lock().map_err(|err| {
            let msg = format!("Failed to obtain dumper-db lock: {}", err);
            DumperDbError::DbLockError { msg }
        })
    }

    pub fn load_account(&self, pubkey: &Pubkey, slot: Slot) -> Result<AccountSharedData, DumperDbError> {
        debug!("Loading account {}", pubkey.to_string());
        let mut client = self.lock_client()?;

        let pubkey_bytes = pubkey.to_bytes();
        let pubkeys = vec!(pubkey_bytes.as_slice());
        let rows = client.query(
            &self.get_accounts_at_slot_statement,
            &[
                &pubkeys,
                &(slot as i64),
            ]
        ).map_err(|err| DumperDbError::LoadAccError { pubkey: pubkey.clone(), slot, err })?;

        if rows.len() != 1 {
            return Err(DumperDbError::TooManyAccs { pubkey: pubkey.clone(), slot });
        }

        Self::read_account(&rows[0])
    }

    pub fn get_block(&self, slot: Slot) -> Result<Block, DumperDbError> {

        debug!("Loading block {}", slot);
        let mut client = self.lock_client()?;
        let rows = client.query(
            &self.get_block_statement,
            &[&(slot as i64)],
        ).map_err(|err| DumperDbError::FailedQueryBlock { slot, err })?;

        if rows.len() != 1 {
            return Err(DumperDbError::NotOneBlock)
        }
        let row = &rows[0];

        let blockhash: String = row.try_get(1)
            .map_err(|err| DumperDbError::FailedReadField { name: "blockhash".to_string() })?;
        let block_time = row.try_get(3)
            .map_or_else(|_| None, |value| Some(value));
        let block_height = row.try_get(4)
            .map_or_else(|_| None, |value| Some(value));

        return Ok(Block {
            slot,
            blockhash,
            block_time,
            block_height,
        })
    }

    pub fn get_transaction_slot(
        &self,
        signature: &Signature
    ) -> Result<Slot, DumperDbError> {

        debug!("Find transaction slot {}", signature);
        let mut client = self.lock_client()?;

        let sign = signature.as_ref().to_vec();
        let rows = client.query(
            &self.find_txn_slot_on_longest_branch_statement,
            &[&sign],
        ).map_err(|err| DumperDbError::FailedQueryTransactionSlot{ signature: signature.clone(), err })?;

        if rows.len() != 1 {
            return Err(DumperDbError::TransactionSlotNotFound { signature: signature.clone() });
        }

        let slot: i64 = rows[0].try_get(0)
            .map_err(|err| DumperDbError::FailedReadField { name: "slot".to_string() })?;
        Ok(slot as u64)
    }

    fn versioned_msg_from_legacy_db_msg(legacy_message: DbTransactionMessage) -> VersionedMessage {
        VersionedMessage::Legacy(LegacyMessage {
            header: MessageHeader {
                num_required_signatures: legacy_message.header.num_required_signatures as u8,
                num_readonly_signed_accounts: legacy_message.header.num_readonly_signed_accounts as u8,
                num_readonly_unsigned_accounts: legacy_message.header.num_readonly_unsigned_accounts as u8,
            },
            account_keys: legacy_message.account_keys
                .iter()
                .map(|entry| Pubkey::new(&entry))
                .collect(),
            recent_blockhash: Hash::new(&legacy_message.recent_blockhash),
            instructions: legacy_message.instructions
                .iter()
                .map(|instr| CompiledInstruction {
                    program_id_index: instr.program_id_index as u8,
                    accounts: instr.accounts.iter().map(|acc| *acc as u8).collect(),
                    data: instr.data.clone(),
                })
                .collect()
        })
    }

    fn versioned_msg_from_v0_db_msg(v0_message: DbTransactionMessageV0) -> VersionedMessage {
        VersionedMessage::V0(V0Message {
            header: MessageHeader {
                num_required_signatures: v0_message.header.num_required_signatures as u8,
                num_readonly_signed_accounts: v0_message.header.num_readonly_signed_accounts as u8,
                num_readonly_unsigned_accounts: v0_message.header.num_readonly_unsigned_accounts as u8,
            },
            account_keys: v0_message.account_keys
                .iter()
                .map(|entry| Pubkey::new(&entry))
                .collect(),
            recent_blockhash: Hash::new(&v0_message.recent_blockhash),
            instructions: v0_message.instructions
                .iter()
                .map(|instr| CompiledInstruction {
                    program_id_index: instr.program_id_index as u8,
                    accounts: instr.accounts.iter().map(|acc| *acc as u8).collect(),
                    data: instr.data.clone(),
                })
                .collect(),
            address_table_lookups: v0_message.address_table_lookups
                .iter()
                .map(|lookup| MessageAddressTableLookup {
                    account_key: Pubkey::new(&lookup.account_key),
                    writable_indexes: lookup.writable_indexes.iter().map(|idx| *idx as u8).collect(),
                    readonly_indexes: lookup.readonly_indexes.iter().map(|idx| *idx as u8).collect(),
                })
                .collect(),
        })
    }

    pub fn get_transaction(
        &self,
        slot: Slot,
        signature: &Signature,
        address_loader: impl AddressLoader
    ) -> Result<SanitizedTransaction, DumperDbError> {

        debug!("Loading transaction {} from slot {}", signature, slot);
        let mut client = self.lock_client()?;

        let sign = signature.as_ref().to_vec();
        let rows = client.query(
            &self.get_transaction_statement,
            &[&(slot as i64), &sign],
        ).map_err(|err| {
            DumperDbError::GetTransactionError{ signature: signature.clone(), slot, err }
        })?;

        if rows.len() == 0 {
            return Err(DumperDbError::TransactionNotFound{ signature: signature.clone(), slot });
        }

        let legacy_message = rows[0].try_get(4);
        let v0_message = rows[0].try_get(5);

        let versioned_message = if legacy_message.is_ok() {
            Some(Self::versioned_msg_from_legacy_db_msg(legacy_message.unwrap()))
        } else if v0_message.is_ok() {
            Some(Self::versioned_msg_from_v0_db_msg(v0_message.unwrap()))
        } else {
            None
        }.map_or_else(
            || Err(DumperDbError::FailedReadMessage{ signature: signature.clone(), slot }),
            |msg| Ok(msg)
        )?;

        let signatures:Vec<Vec<u8>> = rows[0].try_get(6)
            .map_err(|err| DumperDbError::FailedReadField { name: "signatures".to_string() })?;

        let versioned_transaction = SanitizedVersionedTransaction::try_new(
            VersionedTransaction {
                signatures: signatures.iter().map(|sig| Signature::new(&sig)).collect(),
                message: versioned_message,
            })
            .map_err(|err| DumperDbError::FailedCreateVersionedTransaction{ signature: signature.clone(), err })?;

        let is_vote: bool = rows[0].try_get(2)
            .map_err(|err| DumperDbError::FailedReadField { name: "is_vote".to_string() })?;

        let message_hash: Vec<u8> = rows[0].try_get(7)
            .map_err(|err| DumperDbError::FailedReadField { name: "message_hash".to_string() })?;
        let message_hash = Hash::new(&message_hash);

        SanitizedTransaction::try_new(
            versioned_transaction,
            message_hash,
            is_vote,
            address_loader
        )
            .map_err(|err| DumperDbError::FailedCreateSanitizedTransaction { signature: signature.clone(), err })
    }

    fn get_transaction_account_pubkeys(trx: &SanitizedTransaction) -> HashSet<Pubkey> {
        let mut result = HashSet::new();
        match trx.message() {
            SanitizedMessage::Legacy(legacy_message) => {
                legacy_message.account_keys.iter()
                    .for_each(|entry| { result.insert(entry.clone()); });
            }
            SanitizedMessage::V0(v0_message) => {
                v0_message.message.account_keys.iter()
                    .for_each(|entry| { result.insert(entry.clone()); });
                v0_message.loaded_addresses.writable.iter()
                    .for_each(|entry| { result.insert(entry.clone()); });
                v0_message.loaded_addresses.readonly.iter()
                    .for_each(|entry| { result.insert(entry.clone()); });
            }
        }

        result
    }

    pub fn get_transaction_accounts(&self, trx: &SanitizedTransaction) -> Result<BTreeMap<Pubkey, AccountSharedData>, DumperDbError> {

        debug!("Loading accounts for transaction {}", trx.signature());
        let mut client = self.lock_client()?;

        let signature_vec = trx.signature().as_ref().to_vec();
        let accounts = Self::get_transaction_account_pubkeys(trx).iter()
            .map(|entry| entry.to_bytes().as_slice().to_vec()).collect_vec();

        let rows = client.query(
            &self.get_pre_accounts_statement,
            &[&signature_vec, &accounts],
        ).map_err(|err| {
            DumperDbError::FailedGetPreAccounts{ signature: trx.signature().clone(), err }
        })?;

        let mut result = BTreeMap::new();

        for row in rows {
            let account = Self::read_account(&row)?;
            let pubkey: Vec<u8> = row.try_get(5)
                .map_err(|err| DumperDbError::FailedReadField { name: "pubkey".to_string() })?;

            result.insert(Pubkey::new(&pubkey), account);
        }

        Ok(result)
    }

    pub fn get_transaction_and_accounts(
        &self,
        slot: Slot,
        signature: &Signature,
        address_loader: impl AddressLoader
    ) -> Result<(SanitizedTransaction, BTreeMap<Pubkey, AccountSharedData>), DumperDbError> {
        let trx = self.get_transaction(slot, signature, address_loader)?;
        let accounts = self.get_transaction_accounts(&trx)?;
        Ok((trx, accounts))
    }

    pub fn get_recent_blockhashes(&self, start_slot: Slot, num_hashes: u32) -> Result<BTreeMap<u64, String>, DumperDbError> {

        debug!("Loading {} recent blockhashes starting from slot {}", num_hashes, start_slot);
        let mut client = self.lock_client()?;
        let rows = client.query(
            &self.get_recent_blockhashes_statement,
            &[&(start_slot as i64), &(num_hashes as i32)],
        ).map_err(|err| DumperDbError::FailedReadRecentBlockhashes{ start_slot, num_hashes, err })?;

        let mut result = BTreeMap::new();
        for row in rows {
            let slot: i64 = row.try_get(0)
                .map_err(|err| DumperDbError::FailedReadField { name: "slot".to_string() })?;

            let blockhash: String = row.try_get(1)
                .map_err(|err| DumperDbError::FailedReadField { name: "blockhash".to_string() })?;

            result.insert(slot as u64, blockhash);
        }

        Ok(result)
    }
}
