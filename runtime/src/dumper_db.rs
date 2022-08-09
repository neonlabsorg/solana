use {
    postgres::{Client, NoTls, Statement},
};
use solana_sdk::account::AccountSharedData;
use solana_sdk::clock::Slot;
use solana_sdk::pubkey::Pubkey;
use std::sync::{ Arc, Mutex };
use crate::ancestors::Ancestors;
use openssl::ssl::{SslConnector, SslFiletype, SslMethod};
use thiserror::Error;
use log::*;
use postgres_openssl::MakeTlsConnector;

pub struct DumperDb {
    client: Mutex<Client>,
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

    pub fn new(config: &DumperDbConfig) -> Result<Self, DumperDbError> {
        info!("Creating Postgres Client...");
        let mut client = Self::connect_to_db(config)?;

        info!("Created Postgres Client.");
        Ok(Self {
            client: Mutex::new(client),
        })
    }

    pub fn load_account(&self, pubkey: &Pubkey, max_slot: Slot) -> Option<(AccountSharedData, Slot)> {
        todo!()
    }
}

#[derive(Debug, Default)]
pub struct DumperDbBank {
    pub dumper_db: Option<Arc<DumperDb>>,
    pub slot: Slot,
}

impl DumperDbBank {
    pub fn load_account(
        &self,
        ancestors: &Ancestors,
        pubkey: &Pubkey,
        max_root: Option<Slot>
    ) -> Option<(AccountSharedData, Slot)> {
        self.dumper_db.as_ref().unwrap().load_account(pubkey, self.slot)
    }
}