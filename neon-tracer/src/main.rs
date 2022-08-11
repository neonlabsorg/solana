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
    let bank = Bank::new_for_tracer(
        10,
        ClusterType::Development,
        dumper_db.clone(),
        0
    );

    let signature = hex::decode("1148d880712294e08fdb778133e6728618a86cfa2f0908674002851653696356a5634fb3c685c70a12b050d9660c37cc3231e07a30050a4a3dcabe781091b808").unwrap();
    let signature = Signature::new(&signature);
    dumper_db.get_transaction(&signature, &bank);


    return;
}