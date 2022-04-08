use crate::evm_instruction::{verify_tx_signature, SignedTransaction, UnsignedTransaction};
use solana_program_runtime::pre_account::PreAccount;
use evm::H160;
use solana_sdk::account::ReadableAccount;
use solana_sdk::clock::Slot;
use solana_sdk::secp256k1_recover::Secp256k1RecoverError;
use solana_sdk::{
    account::AccountSharedData, keccak, message::Message as SolanaMessage, pubkey::Pubkey,
    signature::Signature,
    message::{SanitizedMessage, legacy}
};

use backoff::{backoff::Backoff, ExponentialBackoff};
use clap::{self, values_t, ArgMatches};
use clickhouse::error::{Error as CHError, Result as CHResult};
use clickhouse::inserter::{Inserter, Quantities};
use clickhouse::{Client, Row};
use evm_loader::instruction::EvmInstruction;
use futures_util::future::OptionFuture;
use generic_array::{typenum::U64, GenericArray};
use hex::ToHex;
use serde::{Serialize, Serializer};
use thiserror::Error;
use tokio::runtime;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Sleep};

use std::collections::VecDeque;
use std::convert::{TryFrom, TryInto};
use std::iter::FromIterator;
use std::num::TryFromIntError;
use std::pin::Pin;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use arrayref::{array_ref};

pub const DEFAULT_DB_URL: &'static str = "http://localhost:8123/";
const DEFAULT_COMMIT_EVERY: Duration = Duration::from_secs(10); // Same as clickhouse crate

struct BufInserterTask<T> {
    rx: Receiver<T>,
    table: &'static str,
    client: Arc<Client>,
    inserter: Inserter<T>,

    buffer: VecDeque<T>,
    backoff: ExponentialBackoff,
    recovery: Option<usize>,
    recovery_delay: Pin<Box<OptionFuture<Sleep>>>,
    commit_delay: Pin<Box<Sleep>>,
    commit_every: Duration,
}

impl<T: Row + Serialize + Send + Sync + 'static> BufInserterTask<T> {
    fn new(rx: Receiver<T>, client: Arc<Client>, table: &'static str) -> Self {
        let inserter = client.inserter(table).expect("can't create inserter");
        let mut backoff = ExponentialBackoff::default();
        backoff.max_elapsed_time = None;

        Self {
            rx,
            client,
            inserter,
            table,
            buffer: VecDeque::new(),
            backoff,
            recovery: None,
            recovery_delay: Box::pin(None.into()),
            commit_delay: Box::pin(sleep(Duration::ZERO)),
            commit_every: DEFAULT_COMMIT_EVERY,
        }
    }

    fn update_idx(&mut self, f: impl FnOnce(&mut usize)) {
        if let Some(idx) = self.recovery.as_mut() {
            f(idx)
        }
    }

    fn restart_recovery(&mut self, err: CHError) {
        let dur = if self.recovery.is_some() {
            log::warn!("{} recovery error: {}", self.table, err);
            self.backoff.next_backoff().unwrap()
        } else {
            log::warn!("{} inserter error: {}, starting recovery", self.table, err);
            Duration::ZERO
        };

        self.recovery_delay.set(Some(sleep(dur)).into());
        self.recovery = Some(0);
    }

    fn proccess_insert_result(&mut self, result: CHResult<Quantities>, just_commit: bool) {
        match result {
            Ok(Quantities::ZERO) if just_commit => (),
            Ok(Quantities::ZERO) => self.update_idx(|idx| *idx += 1),
            Ok(Quantities { entries, .. }) => {
                self.commit_delay.set(sleep(self.commit_every));

                self.buffer.drain(0..(entries as usize));
                self.update_idx(|idx| *idx = 0);

                if self.buffer.is_empty() {
                    if let Some(_idx) = self.recovery.take() {
                        log::info!("finished recovery for table {}", self.table);
                    }
                }
                self.backoff.reset();
            }
            Err(err) => {
                // !: Temp workaround to fix non-zeroing-on-err quantities
                self.inserter = self
                    .client
                    .inserter(self.table)
                    .expect("can't create new inserter");

                self.restart_recovery(err)
            }
        }
    }

    fn is_recovery(&self) -> bool {
        self.recovery.map_or(false, |idx| idx < self.buffer.len())
    }

    async fn run(mut self) {
        macro_rules! write_and_commit {
            ($row:expr) => {
                match self.inserter.write($row).await {
                    Ok(_) => self.inserter.commit().await,
                    Err(err) => Err(err),
                }
            };
        }

        loop {
            tokio::select! {
                Some(entry) = self.rx.recv() => {
                    self.buffer.push_back(entry);
                    if self.recovery.is_none() {
                        let res = write_and_commit!(self.buffer.back().unwrap());
                        self.proccess_insert_result(res, false);
                    }
                }
                _ = &mut self.recovery_delay, if self.is_recovery() => {
                    if let Some(entry) = self.buffer.get(self.recovery.unwrap()) {
                        self.recovery_delay.set(None.into());
                        let res = write_and_commit!(entry);
                        self.proccess_insert_result(res, false);
                    }
                }
                _ = &mut self.commit_delay => {
                    let res = self.inserter.commit().await;
                    self.proccess_insert_result(res, true);
                }
                else => break
            }
        }

        log::info!("stopping insert task for table {}", self.table);
    }
}

struct BufInserter<T> {
    sender: Sender<T>,
    _handle: JoinHandle<()>,
}

impl<T: Row + Serialize + Send + Sync + 'static> BufInserter<T> {
    fn new(client: &Arc<Client>, table: &'static str) -> CHResult<Self> {
        let (sender, rx) = mpsc::channel(1);

        let _handle = tokio::spawn(BufInserterTask::new(rx, client.clone(), table).run());

        Ok(Self { sender, _handle })
    }

    pub async fn insert(&mut self, item: T) {
        self.sender
            .send(item)
            .await
            .unwrap_or_else(|_| panic!("insert task panicked"));
    }
}

#[derive(serde::Serialize, clickhouse::Row, Debug)]
struct AccountsRow {
    date_time: u64,
    transaction_signature: DbSignature,
    public_key: [u8; 32],
    lamports: u64,
    data: Vec<u8>,
    owner: [u8; 32],
    executable: bool,
    rent_epoch: u64,
}

#[derive(serde::Serialize, clickhouse::Row, Debug)]
struct PruneRow {
    slot: Slot,
}

#[derive(serde::Serialize, clickhouse::Row, Debug)]
struct TransactionRow {
    date_time: u64,
    slot: u64,
    transaction_signature: DbSignature,
    message: Vec<u8>,
    logs: Vec<String>,
}

#[derive(serde::Serialize, clickhouse::Row, Debug)]
struct EvmTransactionRow {
    date_time: u64,
    transaction_signature: DbSignature,
    eth_transaction_signature: EthSignature,
    eth_from_addr: [u8; 20],
    eth_to_addr: Option<[u8; 20]>,
}

enum Message {
    AccountsRow(AccountsRow),
    AccountsRowAfterTransaction(AccountsRow),
    PruneRow(PruneRow),
    TransactionRow(TransactionRow),
    EvmTransactionRow(EvmTransactionRow),
}

#[derive(Debug)]
pub struct AccountDumper {
    program_ids: Vec<Pubkey>,
    dump_after_transaction: bool,
    message_tx: Sender<Message>,
}

impl AccountDumper {
    pub fn new(config: Config) -> Self {
        let (message_tx, message_rx) = mpsc::channel(100_000);
        let client = Client::from(config.dumper_db);

        thread::Builder::new()
            .name("account_dumper".into())
            .spawn(|| {
                let runtime = runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                let fut = async move {
                    if let Err(err) = Self::dumper(client, message_rx).await {
                        log::error!("dumper error: {}", err)
                    }
                };

                runtime.block_on(fut)
            })
            .unwrap();

        Self {
            program_ids: config.dumper_program_ids,
            dump_after_transaction: config.dumper_after_transaction,
            message_tx,
        }
    }

    async fn dumper(
        client: Client,
        mut message_rx: mpsc::Receiver<Message>,
    ) -> clickhouse::error::Result<()> {
        let client = Arc::new(client);
        let mut accounts_inserter = BufInserter::new(&client, "accounts")?;
        let mut accounts_after_transaction_inserter =
            BufInserter::new(&client, "accounts_after_transaction")?;
        let mut transactions_inserter = BufInserter::new(&client, "transactions")?;
        let mut evm_transactions_inserter = BufInserter::new(&client, "evm_transactions")?;
        let mut prune_inserter = BufInserter::new(&client, "pruned_transactions")?;

        while let Some(msg) = message_rx.recv().await {
            match msg {
                Message::AccountsRow(row) => {
                    accounts_inserter.insert(row).await;
                }
                Message::AccountsRowAfterTransaction(row) => {
                    accounts_after_transaction_inserter.insert(row).await;
                }
                Message::PruneRow(row) => {
                    prune_inserter.insert(row).await;
                }
                Message::TransactionRow(row) => {
                    transactions_inserter.insert(row).await;
                }
                Message::EvmTransactionRow(row) => {
                    evm_transactions_inserter.insert(row).await;
                }
            }
        }

        Ok(())
    }

    pub fn check_transaction(&self, message: &SanitizedMessage) -> bool {
        match message {
            SanitizedMessage::Legacy(legasy) => {
                legasy
                    .instructions
                    .iter()
                    .filter_map(|ix| legasy.account_keys.get(usize::from(ix.program_id_index)))
                    .any(|program_id| self.program_ids.contains(program_id))
            },
            SanitizedMessage::V0(loaded) => {
                loaded.message
                    .instructions
                    .iter()
                    .filter_map(|ix| loaded.account_keys.get(usize::from(ix.program_id_index)))
                    .any(|program_id| self.program_ids.contains(program_id))
            }
        }
    }

    pub fn account_before_trx(&self, first_signature: &Signature, account: &PreAccount) {
        let row = AccountsRow {
            date_time: db_now(),
            transaction_signature: DbSignature(*first_signature),
            public_key: account.key().to_bytes(),
            lamports: account.lamports(),
            data: account.data().to_vec(),
            owner: account.account().owner().to_bytes(),
            executable: account.executable(),
            rent_epoch: account.account().rent_epoch(),
        };

        log::debug!("account loaded: {:?}", row);

        self.message_tx
            .try_send(Message::AccountsRow(row))
            .unwrap_or_else(|_| panic!("try_send failed"));
    }

    pub fn account_after_trx(
        &self,
        first_signature: &Signature,
        key: &Pubkey,
        shared_data: &AccountSharedData,
    ) {
        if !self.dump_after_transaction {
            return;
        }

        let row = AccountsRow {
            date_time: db_now(),
            transaction_signature: DbSignature(*first_signature),
            public_key: key.to_bytes(),
            lamports: shared_data.lamports(),
            data: shared_data.data().to_vec(),
            owner: shared_data.owner().to_bytes(),
            executable: shared_data.executable(),
            rent_epoch: shared_data.rent_epoch(),
        };

        log::debug!("account changed: {:?}", row);

        self.message_tx
            .try_send(Message::AccountsRowAfterTransaction(row))
            .unwrap_or_else(|_| panic!("try_send failed"));
    }

    pub fn transaction_executed(
        &self,
        slot: u64,
        first_signature: &Signature,
        message: &legacy::Message,
        logs: Vec<String>,
    ) {
        let row = TransactionRow {
            date_time: db_now(),
            slot,
            transaction_signature: DbSignature(*first_signature),
            message: bincode::serialize(message).expect("serialize failed"),
            logs,
        };

        log::debug!("transaction executed: {:?}", row);

        self.message_tx
            .try_send(Message::TransactionRow(row))
            .unwrap_or_else(|_| panic!("try_send failed"));
    }

    pub fn evm_transaction_executed(
        &self,
        evm_ix: EvmInstruction,
        evm_ix_data: &[u8],
        first_signature: &Signature,
        pre_accounts: Vec<&PreAccount>,
    ) {

        let send = |from, signature, unsigned_msg| {
            let row = construct_tx_row( from, unsigned_msg, signature, first_signature);
            //eprintln!("NEON unsigned: {:?} sign: {:?} hash {:?}", unsigned_msg, sign, keccak::hash(&encoded).encode_hex::<String>());
            log::debug!("evm transaction executed: {:?}", row);

            self.message_tx
                .try_send(Message::EvmTransactionRow(row))
                .unwrap_or_else(|_| panic!("try_send failed"))

        };

        match evm_ix {
            EvmInstruction::CallFromRawEthereumTX => {
                let from = H160::from(*array_ref![evm_ix_data, 4, 20]);
                let signature = &evm_ix_data[4 + 20..];
                let unsigned_msg = &evm_ix_data[4 + 20 + 65..];
                send(from, signature, unsigned_msg)
            }
            EvmInstruction::PartialCallOrContinueFromRawEthereumTX => {
                let from = H160::from(*array_ref![evm_ix_data, 4 + 8, 20]);
                let signature = &evm_ix_data[4 + 8 + 20..];
                let unsigned_msg = &evm_ix_data[4 + 8 + 20 + 65..];
                send(from, signature, unsigned_msg)
            }
            EvmInstruction::ExecuteTrxFromAccountDataIterativeOrContinue |
            EvmInstruction::ExecuteTrxFromAccountDataIterativeV02  => {
                if let Some(pre_acc) = pre_accounts.get(0) {
                    let holder_ref = pre_acc.account();
                    let holder = holder_ref.data();

                    match get_transaction_from_holder(holder) {
                        Ok((unsigned_msg, signature)) => {
                            match verify_tx_signature(signature, unsigned_msg){
                                Ok(from) => {
                                    send(from, signature, unsigned_msg)
                                },
                                Err(_) =>  log::warn!("verify_tx_signature error")
                            }
                        },
                        Err(_) => log::warn!("parse holder account error")
                    }
                } else{
                    log::warn!("error while proccessing eth transaction: holder account not found");
                }
            }
            _ =>  log::warn!("unhandled neon instruction {:?}", evm_ix)
        };
    }

    pub fn prune_transaction(&self, prune_slot: Slot) {
        let row = PruneRow { slot: prune_slot };

        log::debug!("transaction pruned: {:?}", row);

        self.message_tx
            .try_send(Message::PruneRow(row))
            .unwrap_or_else(|_| panic!("try_send failed"))
    }
}

fn construct_tx_row(
    from_addr: H160,
    msg: &[u8],
    eth_signature: &[u8],
    sol_signature: &Signature,
) -> EvmTransactionRow {
    let unsigned: UnsignedTransaction = rlp::decode(msg).unwrap();
    let to_addr = unsigned.to;
    let signed = SignedTransaction {
        unsigned,
        signature: eth_signature,
    };
    let encoded = rlp::encode(&signed);

    EvmTransactionRow {
        date_time: db_now(),
        transaction_signature: DbSignature(*sol_signature),
        eth_transaction_signature: EthSignature::new(&encoded),
        eth_from_addr: from_addr.try_into().unwrap(),
        eth_to_addr: to_addr.map(Into::into),
    }
}

#[derive(Debug, Error)]
enum ExtractError {
    #[error("bad holder account tag: {0}")]
    BadAccountKind(u8),
    #[error("bad transaction length: {0}")]
    BadTxLen(#[from] TryFromIntError),
    #[error("holder account not found")]
    NoHolder,
    #[error("failure verifying signature: {0}")]
    BadSignature(#[from] Secp256k1RecoverError),
}

fn get_transaction_from_holder(data: &[u8]) -> Result<(&[u8], &[u8]), ExtractError> {
    let (header, rest) = data.split_at(1);
    if header[0] != 0 {
        // not AccountData::Empty
        return Err(ExtractError::BadAccountKind(header[0]));
    }
    let (signature, rest) = rest.split_at(65);
    let (trx_len, rest) = rest.split_at(8);
    let trx_len = trx_len.try_into().ok().map(u64::from_le_bytes).unwrap();
    let trx_len = usize::try_from(trx_len)?;
    let (trx, _rest) = rest.split_at(trx_len as usize);

    Ok((trx, signature))
}

#[derive(Clone, Default, Debug)]
pub struct Config {
    pub dumper_program_ids: Vec<Pubkey>,
    pub dumper_after_transaction: bool,
    pub dumper_db: DbConfig,
}

impl Config {
    pub fn from_matches(matches: &ArgMatches) -> clap::Result<Self> {
        let dumper_program_ids = values_t!(matches, "dumper_program_ids", Pubkey)?;
        let dumper_after_transaction = matches.is_present("dumper_after_transaction");
        let dumper_db = DbConfig::from_matches(matches)?;

        Ok(Self {
            dumper_program_ids,
            dumper_after_transaction,
            dumper_db,
        })
    }
}

#[derive(Clone, Debug)]
pub struct DbConfig {
    pub dumper_db_url: String,
    pub dumper_db_database: Option<String>,
    pub dumper_db_user: Option<String>,
    pub dumper_db_password: Option<String>,
}

impl DbConfig {
    pub fn from_matches(matches: &ArgMatches) -> clap::Result<Self> {
        let dumper_db_url = matches
            .value_of("dumper_db_url")
            .unwrap_or(DEFAULT_DB_URL)
            .to_string();
        let dumper_db_database = matches.value_of("dumper_db_database").map(String::from);
        let dumper_db_user = matches.value_of("dumper_db_user").map(String::from);
        let dumper_db_password = matches.value_of("dumper_db_password").map(String::from);

        Ok(Self {
            dumper_db_url,
            dumper_db_database,
            dumper_db_user,
            dumper_db_password,
        })
    }
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            dumper_db_url: DEFAULT_DB_URL.to_string(),
            dumper_db_database: None,
            dumper_db_user: None,
            dumper_db_password: None,
        }
    }
}

impl From<DbConfig> for Client {
    fn from(config: DbConfig) -> Self {
        let mut client = Client::default().with_url(config.dumper_db_url);

        if let Some(database) = config.dumper_db_database {
            client = client.with_database(database);
        }

        if let Some(user) = config.dumper_db_user {
            client = client.with_user(user);
        }

        if let Some(password) = config.dumper_db_password {
            client = client.with_password(password);
        }

        client
    }
}

#[derive(Debug)]
struct DbSignature(Signature);

impl Serialize for DbSignature {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeTuple;

        let mut seq = serializer.serialize_tuple(64)?;
        for e in self.0.as_ref() {
            seq.serialize_element(e)?;
        }
        seq.end()
    }
}

#[derive(
    serde::Serialize, serde::Deserialize, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug,
)]
struct EthSignature(GenericArray<u8, U64>);

impl EthSignature {
    fn new(sign: &[u8]) -> Self {
        let hash = keccak::hash(sign);
        hash.encode_hex()
    }

    /*
    fn from_tnx(raw_tnx: &[u8]) -> Self {
        let hex = Vec::from_hex(raw_tnx).unwrap();
        let hash = keccak::hash(hex.as_slice());
        hash.encode_hex()
    }
    */
}

impl FromIterator<char> for EthSignature {
    fn from_iter<I: IntoIterator<Item = char>>(iter: I) -> Self {
        let iter = iter.into_iter().map(|ch| ch as u8); // only hex characters
        let array = GenericArray::from_exact_iter(iter).expect("wrong size iterator");
        Self(array)
    }
}

fn db_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .try_into()
        .unwrap()
}
