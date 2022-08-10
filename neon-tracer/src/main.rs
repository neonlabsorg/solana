use std::sync;
use {
    solana_runtime::{
        bank::Bank,
        dumper_db::{ DumperDb, DumperDbConfig }
    },
    sync::Arc,
    solana_sdk::genesis_config::ClusterType,
};

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

    let dumper_db = DumperDb::new(&config).unwrap();
    let bank = Bank::new_for_tracer(
        0,
        ClusterType::Development,
        Arc::new(dumper_db),
        0
    );
    return;
}