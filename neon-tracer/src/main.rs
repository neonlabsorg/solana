use {
    clap::{ App, Arg, AppSettings, crate_description, crate_name, SubCommand },
    handlebars::Handlebars,
    log::*,
    solana_runtime::{
        bank::{ Bank, TransactionSimulationResult },
        dumper_db::{ DumperDb, DumperDbConfig, DumperDbError },
        neon_tracer_bank::BankCreationError,
    },
    std::{ collections::HashMap, str::FromStr, sync::Arc },
    solana_sdk::{ clock::Slot, genesis_config::ClusterType, signature::Signature },
    thiserror::Error,
};

macro_rules! neon_tracer_pkg_version {
    () => ( env!("CARGO_PKG_VERSION") )
}

macro_rules! neon_tracer_revision {
    () => ( env!("NEON_TRACER_REVISION") )
}

macro_rules! version_string {
    () => ( concat!("Neon-tracer/v", neon_tracer_pkg_version!(), "-", neon_tracer_revision!()) )
}

#[derive(Debug, Error)]
pub enum TracerError {
    #[error("Failed to create DumperDb: {err}")]
    FailedCreateDumperDb{ err: DumperDbError },

    #[error("Failed to query transaction {signature}: {err}")]
    FailedToGetSlot{ signature: Signature, err: DumperDbError },

    #[error("Failed to create bank {slot} slot: {err}")]
    FailedCreateBank{ slot: Slot, err: BankCreationError },

    #[error("Failed to query transaction and accounts {signature}: {err}")]
    FailedQueryTransactionAccounts{ signature: Signature, err: DumperDbError },

    #[error("Failed to simulate transaction {signature}")]
    FailedSimulateTransaction{ signature: Signature },
}

// Return an error if string cannot be parsed as a Base58 encoded Solana signature
fn is_valid_signature<T>(string: T) -> Result<(), String> where T: AsRef<str> {
    Signature::from_str(string.as_ref()).map(|_| ())
        .map_err(|e| e.to_string())
}

// Return an error if string cannot be parsed as output format
fn is_valid_out_format<T>(string: T) -> Result<(), String> where T: AsRef<str>,
{
    Ok(())
}

pub fn create_dumperdb(db_config: &DumperDbConfig) -> Result<Arc<DumperDb>, TracerError> {
    Ok(Arc::new(DumperDb::new(db_config)
        .map_err(|err| TracerError::FailedCreateDumperDb{ err })?))
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

    bank.replay_transaction(trx, &accounts).map_or(
        Err(TracerError::FailedSimulateTransaction { signature: signature.clone() }),
        |result| Ok(result)
    )
}

pub fn main() {
    solana_logger::setup();

    let app_matches = App::new(crate_name!())
        .about(crate_description!())
        .version(version_string!())
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(Arg::with_name("connection-str")
                .short("c")
                .long("connection-str")
                .value_name("CONN_STR")
                .takes_value(true)
                .global(true)
                .help("Dumper DB connection string"))
        .subcommand(SubCommand::with_name("replay")
            .about("Replay Solana transaction given signature")
            .arg(
                Arg::with_name("signature")
                    .value_name("SIGNATURE")
                    .takes_value(true)
                    .index(1)
                    .required(true)
                    .validator(is_valid_signature)
                    .help("Signature of Solana transaction")
            )
            .arg(
                Arg::with_name("output-format")
                    .value_name("OUTPUT_FORMAT")
                    .takes_value(true)
                    .index(2)
                    .required(false)
                    .validator(is_valid_out_format)
                    .help("Output format of command (compatible with handlebars templater). Supported placeholders: result, logs, units_consumed, return")
            ))
        .get_matches();

    let config = DumperDbConfig {
        port: None,
        connection_str: Some(app_matches.value_of("connection-str").unwrap().to_string()),
        host: None,
        user: None,
        use_ssl: None,
        server_ca: None,
        client_cert: None,
        client_key: None,
    };

    let dumper_db = create_dumperdb(&config).unwrap();

    let (sub_command, sub_matches) = app_matches.subcommand();
    match (sub_command, sub_matches) {
        ("replay", Some(arg_matches)) => {
            let signature = Signature::from_str(arg_matches.value_of("signature").unwrap()).unwrap();
            let simulation_result = replay_transaction(
                dumper_db,
                ClusterType::Development,
                &signature,
                true).unwrap();

            let output_format = arg_matches.value_of("output-format")
                .unwrap_or(
                    "Simulation result: {{{ result }}}\nLog messages: {{{ logs }}}\nUnits consumed: {{{ units_consumed }}}\nReturn: {{{ return }}}"
                );

            let mut handlebars = Handlebars::new();
            handlebars
                .register_template_string("ouput_format", output_format)
                .unwrap();

            let mut data = HashMap::new();
            data.insert("result", format!("{:?}", simulation_result.result));
            data.insert("logs", format!("{:?}", simulation_result.logs));
            data.insert("units_consumed", format!("{:?}", simulation_result.units_consumed));
            data.insert("return", format!("{:?}", simulation_result.return_data));

            print!("{}", handlebars.render("ouput_format", &data).unwrap());
        }
        (_, _) => {}
    }
}
