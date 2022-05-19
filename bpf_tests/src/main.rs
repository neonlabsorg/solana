mod read_elf;
mod vm;
mod evm_instructions;
mod program_options;

use std::{
    env,
    process::{exit},
};
use structopt::StructOpt;

use evm_instructions::create_account_v02;


fn main(){
    solana_logger::setup();

    let mut args = env::args().collect::<Vec<_>>();
    if let Some("run-bpf-tests") = args.get(1).map(|a| a.as_str()) {
        args.remove(1);
    }

    let opt = program_options::Opt::from_iter(&args);
    if let Err(e) = create_account_v02::process(&opt) {
        eprintln!("error: {:#}", e);
        exit(1);
    }

}
