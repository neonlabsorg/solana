use {
    solana_runtime::{
        bank::Bank,
        dumper_db::{ DumperDb, DumperDbConfig }
    },
    std::sync::Arc,
    solana_sdk::genesis_config::ClusterType,
};
use solana_sdk::signature::Signature;
use std::str::FromStr;
use hex;
use log::*;
use solana_ledger::builtins::get;


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



    let dumper_db = Arc::new(DumperDb::new(&config).unwrap());

    let signature = hex::decode("913b4284f6da45241272234cf90748da782e1106df34e0375fa8a8fba1f4c4649ea5af2266fa27fc9a39493b0e7059db1660f359dfea342ab8fb0c71e717890f").unwrap();
    let signature = Signature::new(&signature);
    let slot = dumper_db.get_transaction_slot(&signature).unwrap();

    let bank = Bank::new_for_tracer(
        slot,
        ClusterType::Development,
        dumper_db.clone(),
        0,
        Some(&solana_ledger::builtins::get(true)),
    ).unwrap();

    let (trx, accounts) = dumper_db.get_transaction_and_accounts(slot, &signature, &bank).unwrap();
    debug!("message: {:?}, accounts: {:?}", trx.message(), accounts);

    bank.dumper_db().load_accounts_to_cache(&accounts);
    bank.set_enable_loading_from_dumper_db(false);
    let simulation_result = bank.simulate_transaction(trx);

    debug!("Simulation finished:");
    debug!("Simulation result: {:?}", simulation_result.result);
    debug!("Log messages: {:?}", simulation_result.logs);
    debug!("Units consumed: {}", simulation_result.units_consumed);
}