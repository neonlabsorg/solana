mod read_elf;
// mod vm;
mod vm_1_9_12;
mod evm_loader;
mod program_options;

use std::{
    env,
    process::{exit},
};
use structopt::StructOpt;


fn main(){
    solana_logger::setup();

    let mut args = env::args().collect::<Vec<_>>();
    if let Some("run-bpf-tests") = args.get(1).map(|a| a.as_str()) {
        args.remove(1);
    }

    let opt = program_options::Opt::from_iter(&args);
    if let Err(e) = evm_loader::create_account(&opt) {
        eprintln!("error: {:#}", e);
        exit(1);
    }

}
