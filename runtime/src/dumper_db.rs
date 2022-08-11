use {
    crate::ancestors::Ancestors,
    log::*,
    neon_dumper_plugin::postgres_client::postgres_client_transaction::{
        DbTransactionMessage,
        DbTransactionMessageV0
    },
    openssl::ssl::{SslConnector, SslFiletype, SslMethod},
    postgres::{Client, NoTls, Row, row::RowIndex, Statement},
    postgres_openssl::MakeTlsConnector,
    postgres_types::FromSql,
    solana_sdk::{
        account::{ AccountSharedData, WritableAccount },
        clock::Slot, pubkey::Pubkey,
        message::Message as LegacyMessage,
        message::v0::Message as V0Message,
        signature::Signature,
        transaction::SanitizedTransaction,
        hash::Hash,
    },
    std::{ collections::BTreeMap, sync::{ Arc, Mutex }},
    thiserror::Error,
};
use solana_sdk::instruction::CompiledInstruction;
use solana_sdk::message::{MessageHeader, VersionedMessage};
use solana_sdk::message::v0::MessageAddressTableLookup;
use solana_sdk::transaction::{SanitizedVersionedTransaction, VersionedTransaction, AddressLoader};

pub struct DumperDb {
    client: Mutex<Client>,
    get_accounts_at_slot_statement: Statement,
    get_block_statement: Statement,
    get_transaction_statement: Statement,
    get_transaction_slots_statement: Statement,
    get_pre_accounts_statement: Statement,
    get_recent_blockhashes_statement: Statement,
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

    fn create_get_transaction_slots_statement(client: &mut Client) -> Result<Statement, DumperDbError> {
        let stmt = "SELECT slot FROM transaction WHERE position($1 in signature) > 0;";
        let stmt = client.prepare(stmt);
        stmt.map_err(|err| {
            let msg = format!("Failed to prepare get_transaction_slots statement: {}", err);
            DumperDbError::Custom { msg }
        })
    }

    fn create_get_pre_accounts_statement(client: &mut Client) -> Result<Statement, DumperDbError> {
        let stmt = "SELECT lamports, data, owner, executable, rent_epoch, pubkey FROM get_pre_accounts($1);";
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

    pub fn new(config: &DumperDbConfig) -> Result<Self, DumperDbError> {
        info!("Creating Postgres Client...");
        let mut client = Self::connect_to_db(config)?;

        let get_accounts_at_slot_statement = Self::create_get_accounts_at_slot_statement(&mut client)?;
        let get_block_statement = Self::create_get_block_statement(&mut client)?;
        let get_transaction_statement = Self::create_get_transaction_statement(&mut client)?;
        let get_transaction_slots_statement = Self::create_get_transaction_slots_statement(&mut client)?;
        let get_pre_accounts_statement = Self::create_get_pre_accounts_statement(&mut client)?;
        let get_recent_blockhashes_statement = Self::create_get_recent_blockhashes_statement(&mut client)?;

        info!("Created Postgres Client.");
        Ok(Self {
            client: Mutex::new(client),
            get_accounts_at_slot_statement,
            get_block_statement,
            get_transaction_statement,
            get_transaction_slots_statement,
            get_pre_accounts_statement,
            get_recent_blockhashes_statement,
        })
    }

    fn read_field<'a, T, I>(row: &'a Row, field_number: I, field_name: &str) -> Option<T>
    where
        I: RowIndex + std::fmt::Display,
        T: FromSql<'a>
    {
        let value = row.try_get(field_number);
        if let Err(err) = value {
            error!(
                "Failed to read '{}' field: {}",
                field_name,
                err,
            );
            return None;
        }
        let value: T = value.unwrap();
        Some(value)
    }

    fn read_account(row: &Row) -> Option<AccountSharedData> {
        let lamports = Self::read_field::<i64, _>(row, 0, "lamports");
        if lamports.is_none() {
            return None;
        }

        let data = Self::read_field(row, 1, "data");
        if data.is_none() {
            return None;
        }

        let owner = Self::read_field::<&[u8], _>(row, 2, "owner");
        if owner.is_none() {
            return None;
        }

        let executable = Self::read_field::<bool, _>(row, 3, "executable");
        if executable.is_none() {
            return None;
        }

        let rent_epoch = Self::read_field::<i64, _>(row, 4, "rent_epoch");
        if rent_epoch.is_none() {
            return None;
        }

        let account = AccountSharedData::create(
            lamports.unwrap() as u64,
            data.unwrap(),
            Pubkey::new(owner.unwrap()),
            executable.unwrap(),
            rent_epoch.unwrap() as u64
        );

        return Some(account);
    }

    pub fn load_account(&self, pubkey: &Pubkey, slot: Slot) -> Option<AccountSharedData> {
        debug!("Loading account {}", pubkey.to_string());

        let mut client = self.client.lock();
        if let Err(err) = client {
            let msg = format!("Failed to obtain dumper-db lock: {}", err);
            error!("{}", msg);
            return None;
        }

        let mut client = client.unwrap();
        let pubkey_bytes = pubkey.to_bytes();
        let pubkeys = vec!(pubkey_bytes.as_slice());
        let result = client.query(
            &self.get_accounts_at_slot_statement,
            &[
                &pubkeys,
                &(slot as i64),
            ]
        );

        if let Err(err) = result {
            let msg = format!("Failed to load account: {}", err);
            error!("{}", msg);
            return None;
        }

        let rows = result.unwrap();
        if rows.len() != 1 {
            error!("More than one occurrences of account {} found!", pubkey.to_string());
            return None;
        }

        Self::read_account(&rows[0])
    }

    pub fn get_block(&self, slot: Slot) -> Option<Block> {
        debug!("Loading block {}", slot);
        let mut client = self.client.lock();
        if let Err(err) = client {
            let msg = format!("Failed to obtain dumper-db lock: {}", err);
            error!("{}", msg);
            return None;
        }

        let mut client = client.unwrap();
        let result = client.query(
            &self.get_block_statement,
            &[&(slot as i64)],
        );

        if let Err(err) = result {
            let msg = format!("Failed to load block: {}", err);
            error!("{}", msg);
            return None;
        }

        let rows = result.unwrap();
        if rows.len() > 1 {
            error!("More than one occurrences of block {} found!", slot);
            return None;
        } else if rows.len() < 1 {
            error!("Block {} not found!", slot);
            return None;
        }
        let row = &rows[0];

        let blockhash = Self::read_field::<String, _>(row, 1, "blockhash");
        if blockhash.is_none() {
            return None;
        }
        let blockhash = blockhash.unwrap();
        info!("Blockhash: {}", &blockhash);

        let block_time = Self::read_field::<i64, _>(row, 3, "block_time");
        let block_height = Self::read_field::<i64, _>(row, 4, "block_height");
        return Some(Block {
            slot,
            blockhash: blockhash,
            block_time,
            block_height,
        })
    }

    pub fn get_transaction_slots(
        &self,
        signature: &Signature
    ) -> Option<Vec<Slot>> {
        debug!("Loading transaction slots {}", signature);
        let mut client = self.client.lock();
        if let Err(err) = client {
            let msg = format!("Failed to obtain dumper-db lock: {}", err);
            error!("{}", msg);
            return None;
        }

        let mut client = client.unwrap();
        let sign = signature.as_ref().to_vec();
        let result = client.query(
            &self.get_transaction_slots_statement,
            &[&sign],
        );

        if let Err(err) = result {
            let msg = format!("Failed to load transaction slots: {}", err);
            error!("{}", msg);
            return None;
        }

        let rows = result.unwrap();
        if rows.len() == 0 {
            let msg = format!("Transaction {} not found", signature);
            error!("{}", msg);
            return None;
        }

        let mut slots = Vec::new();
        for row in rows {
            let slot = row.try_get(0);
            if slot.is_err() {
                let msg = format!("Failed to read slot for transaction {}", signature);
                error!("{}", msg);
                return None;
            }
            let slot: i64 = slot.unwrap();
            slots.push(slot as u64);
        }

        Some(slots)
    }

    pub fn get_transaction(
        &self,
        slot: Slot,
        signature: &Signature,
        address_loader: impl AddressLoader
    ) -> Option<SanitizedTransaction> {

        debug!("Loading transaction {} from slot {}", signature, slot);
        let mut client = self.client.lock();
        if let Err(err) = client {
            let msg = format!("Failed to obtain dumper-db lock: {}", err);
            error!("{}", msg);
            return None;
        }

        let mut client = client.unwrap();
        let sign = signature.as_ref().to_vec();
        let result = client.query(
            &self.get_transaction_statement,
            &[&(slot as i64), &sign],
        );

        if let Err(err) = result {
            let msg = format!("Failed to load transaction: {}", err);
            error!("{}", msg);
            return None;
        }

        let rows = result.unwrap();
        if rows.len() == 0 {
            let msg = format!("Transaction {} not found", signature);
            error!("{}", msg);
            return None;
        }

        let legacy_message = rows[0].try_get(4);
        let v0_message = rows[0].try_get(5);

        let versioned_message = if legacy_message.is_ok() {
            let legacy_message: DbTransactionMessage = legacy_message.unwrap();
            let legacy_message = LegacyMessage {
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
            };

            Some(VersionedMessage::Legacy(legacy_message))

        } else if v0_message.is_ok() {
            let v0_message: DbTransactionMessageV0 = v0_message.unwrap();
            let v0_message = V0Message {
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
            };

            Some(VersionedMessage::V0(v0_message))

        } else {
            return None
        };

        if versioned_message.is_none() {
            let msg = format!("Empty transaction record in db for signature: {}", signature);
            error!("{}", msg);
            return None;
        }

        let signatures = rows[0].try_get(6);
        if signatures.is_err() {
            let msg = format!("Unable to read transaction signatures {}", signature);
            error!("{}", msg);
            return None;
        }
        let signatures: Vec<Vec<u8>> = signatures.unwrap();
        let versioned_transaction = VersionedTransaction {
            signatures: signatures.iter().map(|sig| Signature::new(&sig)).collect(),
            message: versioned_message.unwrap(),
        };
        let versioned_transaction = SanitizedVersionedTransaction::try_new(versioned_transaction);
        if versioned_transaction.is_err() {
            let msg = format!("Unable to create SanitizedVersionedTransaction {}", signature);
            error!("{}", msg);
            return None;
        }

        let is_vote = rows[0].try_get(2);
        if is_vote.is_err() {
            let msg = format!("Unable to read is_vote field for trx {}", signature);
            error!("{}", msg);
            return None;
        }
        let is_vote: bool = is_vote.unwrap();

        let message_hash = rows[0].try_get(7);
        if message_hash.is_err() {
            let msg = format!("Unable to read message_hash {}", signature);
            error!("{}", msg);
            return None;
        }
        let message_hash: Vec<u8> = message_hash.unwrap();
        let message_hash = Hash::new(&message_hash);

        let result = SanitizedTransaction::try_new(
            versioned_transaction.unwrap(),
            message_hash,
            is_vote,
            address_loader
        );

        if result.is_err() {
            let msg = format!("Failed to create SanitizedTransaction {}", signature);
            error!("{}", msg);
            return None;
        }

        return Some(result.unwrap());
    }

    pub fn get_transaction_accounts(&self, signature: &Signature) -> Option<BTreeMap<Pubkey, AccountSharedData>> {

        debug!("Loading accounts for transaction {}", signature);
        let mut client = self.client.lock();
        if let Err(err) = client {
            let msg = format!("Failed to obtain dumper-db lock: {}", err);
            error!("{}", msg);
            return None;
        }

        let mut client = client.unwrap();
        let signature = signature.as_ref().to_vec();
        let result = client.query(
            &self.get_pre_accounts_statement,
            &[&signature],
        );

        if let Err(err) = result {
            let msg = format!("Failed to load accounts: {}", err);
            error!("{}", msg);
            return None;
        }

        let rows = result.unwrap();
        if rows.len() == 0 {
            let msg = format!("Accounts not found");
            error!("{}", msg);
            return None;
        }

        let mut result = BTreeMap::new();

        for row in rows {
            let account = Self::read_account(&row);
            if account.is_none() {
                let msg = format!("Failed to read account");
                error!("{}", msg);
                return None;
            }

            if let Some(pubkey) = Self::read_field::<Vec<u8>, _>(&row, 5, "pubkey") {
                let pubkey = Pubkey::new(&pubkey);
                debug!("   Account loaded: {}", &pubkey);
                result.insert(pubkey, account.unwrap());
            } else {
                let msg = format!("Failed read account pubkey");
                error!("{}", msg);
                return None;
            }
        }

        Some(result)
    }

    pub fn get_transaction_and_accounts(
        &self,
        slot: Slot,
        signature: &Signature,
        address_loader: impl AddressLoader
    ) -> Option<(SanitizedTransaction, BTreeMap<Pubkey, AccountSharedData>)> {
        let trx = self.get_transaction(slot, signature, address_loader);
        if trx.is_none() {
            return None;
        }
        let trx = trx.unwrap();

        let accounts = self.get_transaction_accounts(trx.signature());
        if accounts.is_none() {
            return None;
        }

        Some((trx, accounts.unwrap()))
    }

    pub fn get_recent_blockhashes(&self, start_slot: Slot, num_hashes: u32) -> Option<BTreeMap<u64, String>> {
        debug!("Loading {} recent blockhashes starting from slot {}", num_hashes, start_slot);
        let mut client = self.client.lock();
        if let Err(err) = client {
            let msg = format!("Failed to obtain dumper-db lock: {}", err);
            error!("{}", msg);
            return None;
        }

        let mut client = client.unwrap();
        let result = client.query(
            &self.get_recent_blockhashes_statement,
            &[&(start_slot as i64), &(num_hashes as i32)],
        );

        if let Err(err) = result {
            let msg = format!("Failed to get recent blockhashes: {}", err);
            error!("{}", msg);
            return None;
        }

        let rows = result.unwrap();
        if rows.len() == 0 {
            let msg = format!("Blockhashes not found");
            error!("{}", msg);
            return None;
        }

        let mut result = BTreeMap::new();
        for row in rows {
            let slot = Self::read_field::<i64, _>(&row, 0, "slot");
            if slot.is_none() {
                let msg = format!("Failed to read slot from row");
                error!("{}", msg);
                return None;
            }

            let blockhash = Self::read_field::<String, _>(&row, 1, "blockhash");
            if blockhash.is_none() {
                let msg = format!("Failed to read blockhash from row");
                error!("{}", msg);
                return None;
            }

            result.insert(slot.unwrap() as u64, blockhash.unwrap());
        }

        Some(result)
    }
}

#[derive(Debug, Default)]
pub struct EnableLoading{
    pub value: bool
}

impl EnableLoading {
    pub fn set_enable_loading(&mut self, enable: bool) {
        self.value = enable;
    }
}

#[derive(Debug, Default)]
pub struct DumperDbBank {
    pub dumper_db: Option<Arc<DumperDb>>,
    pub slot: Slot,
    pub account_cache: Mutex<BTreeMap<Pubkey, AccountSharedData>>,
    pub enable_loading_from_db: Mutex<EnableLoading>,
}

impl DumperDbBank {
    pub fn new(dumper_db: Arc<DumperDb>, slot: Slot) -> Self {
        DumperDbBank {
            dumper_db: Some(dumper_db),
            slot,
            account_cache: Mutex::new(BTreeMap::new()),
            enable_loading_from_db: Mutex::new(EnableLoading{ value: true }),
        }
    }

    pub fn set_enable_loading_from_dumper_db(&self, enable: bool) {
        if let Ok(mut enable_loading) = self.enable_loading_from_db.lock() {
            enable_loading.set_enable_loading(enable);
        }
    }

    pub fn take_cache_snapshot(&self) -> Option<BTreeMap<Pubkey, AccountSharedData>> {
        let account_cache = self.account_cache.lock();
        match account_cache {
            Err(err) => {
                let msg = format!("Failed to obtain account-cache lock: {}", err);
                error!("{}", msg);
                None
            }
            Ok(mut account_cache) => {
                Some(account_cache.clone())
            }
        }
    }

    pub fn clear_cache(&self) -> bool {
        let account_cache = self.account_cache.lock();
        match account_cache {
            Err(err) => {
                let msg = format!("Failed to obtain account-cache lock: {}", err);
                error!("{}", msg);
                false
            }
            Ok(mut account_cache) => {
                account_cache.clear();
                debug!("Account cache cleared");
                true
            }
        }
    }

    pub fn load_accounts_to_cache(&self, snapshot: &BTreeMap<Pubkey, AccountSharedData>) -> bool {
        let account_cache = self.account_cache.lock();
        match account_cache {
            Err(err) => {
                let msg = format!("Failed to obtain account-cache lock: {}", err);
                error!("{}", msg);
                false
            }
            Ok(mut account_cache) => {
                for (pubkey, account) in snapshot {
                    account_cache.insert(pubkey.clone(), account.clone());
                }
                debug!("Account cache: Loading finished");
                true
            }
        }
    }

    pub fn load_account(
        &self,
        ancestors: &Ancestors,
        pubkey: &Pubkey,
        max_root: Option<Slot>
    ) -> Option<(AccountSharedData, Slot)> {
        let account_cache = self.account_cache.lock();
        let enable_loading = self.enable_loading_from_db.lock();
        if enable_loading.is_err() {
            return None;
        }
        match account_cache {
            Err(err) => {
                let msg = format!("Failed to obtain account-cache lock: {}", err);
                error!("{}", msg);
                return None;
            }
            Ok(mut account_cache) => {
                if let Some(account) = account_cache.get(pubkey) {
                    debug!("Account {} found in cache", pubkey);
                    return Some((account.clone(), self.slot))
                }

                if enable_loading.unwrap().value {
                    if let Some(account) = self.dumper_db.as_ref().unwrap().load_account(pubkey, self.slot) {
                        debug!("Account {} loaded from DB", pubkey);
                        account_cache.insert(*pubkey, account.clone());
                        return Some((account, self.slot))
                    }
                }

                let msg = format!("Unable to read account {} from dumper-db", pubkey.to_string());
                error!("{}", msg);
                None
            }
        }
    }
}