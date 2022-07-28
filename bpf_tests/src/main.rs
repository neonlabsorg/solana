mod read_elf;
mod vm;
mod evm_instructions;
mod tracing;

use std::process::exit;

#[allow(unused_imports)]
use evm_instructions::{
    create_account_v02,
    call_from_raw_ethereum_tx,
};
use tracing::Tracer;
use solana_bpf_loader_program::syscalls as syscalls;

fn main(){
    solana_logger::setup();

    let mut tracer = Tracer::new();
    syscalls::using(&mut tracer, ||{

        // if let Err(e) = create_account_v02::process() {
        //     eprintln!("error: {:#}", e);
        //     exit(1);
        // }

        if let Err(e) = call_from_raw_ethereum_tx::process() {
            eprintln!("error: {:#}", e);
            exit(1);
        }
    })
}
