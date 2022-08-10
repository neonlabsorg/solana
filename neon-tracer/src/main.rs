use std::sync;
use {
    solana_runtime::{
        accounts_db::AccountsDb,
        bank::Bank,
        dumper_db::{ DumperDb, DumperDbConfig }
    },
    sync::Arc,
};
use solana_runtime::dumper_db::DumperDbBank;
use solana_runtime::inline_spl_token_2022::Account;

pub fn main() {
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
    let dumper_db = DumperDb::new(&config).unwrap();
    let mut accounts_db = AccountsDb::default_for_tests();
    accounts_db.dumper_db = DumperDbBank::new(
        Arc::new(dumper_db),
        0
    );
    let bank = Bank::new_for_tracer(accounts_db, 0);
    return;
}