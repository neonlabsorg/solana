use {
    solana_runtime::{
        bank::{ Bank, TransactionSimulationResult },
        dumper_db::{ DumperDb, DumperDbConfig, DumperDbError },
        neon_tracer_bank::BankCreationError,
    },
    std::sync::Arc,
    solana_sdk::{ clock::Slot, genesis_config::ClusterType, },
    thiserror::Error,
};
use solana_sdk::signature::Signature;
use std::str::FromStr;
use hex;
use log::*;
use solana_ledger::builtins::get;

#[derive(Debug, Error)]
pub enum TracerError {
    #[error("Failed to create DumperDb")]
    FailedCreateDumperDb,

    #[error("Failed to query transaction {signature}: {err}")]
    FailedToGetSlot{ signature: Signature, err: DumperDbError },

    #[error("Failed to create bank {slot} slot: {err}")]
    FailedCreateBank{ slot: Slot, err: BankCreationError },

    #[error("Failed to query transaction and accounts {signature}: {err}")]
    FailedQueryTransactionAccounts{ signature: Signature, err: DumperDbError },
}

pub fn create_dumperdb(db_config: &DumperDbConfig) -> Result<Arc<DumperDb>, TracerError> {
    Ok(Arc::new(DumperDb::new(db_config)
        .map_err(|err| TracerError::FailedCreateDumperDb)?))
}

pub fn replay_transaction(
    dumper_db: Arc<DumperDb>,
    cluster_type: ClusterType,
    signature: &Signature,
    bpf_jit: bool,
) -> Result<TransactionSimulationResult, TracerError> {
    let slot = dumper_db.get_transaction_slot(signature)
        .map_err(|err| TracerError::FailedToGetSlot { signature: signature.clone(), err })?;

    let bank = Bank::new_for_tracer(
        slot,
        cluster_type,
        dumper_db.clone(),
        0,
        Some(&solana_ledger::builtins::get(bpf_jit))
    ).map_err(|err| TracerError::FailedCreateBank { slot, err })?;

    let (trx, accounts) = dumper_db
        .get_transaction_and_accounts(slot, signature, &bank)
        .map_err(|err| TracerError::FailedQueryTransactionAccounts { signature: signature.clone(), err })?;

    bank.dumper_db().load_accounts_to_cache(&accounts);
    bank.set_enable_loading_from_dumper_db(false);
    Ok(bank.simulate_transaction(trx))
}

pub fn main() {
    solana_logger::setup();

    let config = DumperDbConfig {
        port: None,
        connection_str: Some("host=localhost dbname=solana user=solana-user port=5432 password=solana-pass".to_string()),
        host: None,
        user: None,
        use_ssl: None,
        server_ca: None,
        client_cert: None,
        client_key: None,
    };

    let dumper_db = create_dumperdb(&config).unwrap();

    let signature = hex::decode("913b4284f6da45241272234cf90748da782e1106df34e0375fa8a8fba1f4c4649ea5af2266fa27fc9a39493b0e7059db1660f359dfea342ab8fb0c71e717890f").unwrap();
    let signature = Signature::new(&signature);
    let simulation_result = replay_transaction(
        dumper_db,
        ClusterType::Development,
        &signature,
        true).unwrap();

    debug!("Simulation finished:");
    debug!("Simulation result: {:?}", simulation_result.result);
    debug!("Log messages: {:?}", simulation_result.logs);
    debug!("Units consumed: {}", simulation_result.units_consumed);
}