use evm_loader::account::EthereumAccount;
use crate::read_elf;
use crate::program_options;
use crate::vm;
use bincode::serialize;

use evm::{H160};
use evm_loader::account::ACCOUNT_SEED_VERSION;


use solana_sdk::{
    account::AccountSharedData,
    bpf_loader,
    native_loader,
    pubkey::Pubkey,
    system_program,
};
use std::{
    str::FromStr,
    cell::RefCell,
    rc::Rc,
};
use solana_sdk::account::WritableAccount;

use crate::evm_instructions::{
    feature_set,
    bpf_loader_shared,
    evm_loader_shared,
    system_shared,
    evm_loader_str
};



pub fn process(
    opt: &program_options::Opt
) -> Result<(), anyhow::Error> {

    let evm_contract = read_elf::read_so(opt)?;

    let evm_loader = Pubkey::from_str(&evm_loader_str)?;

    let ether_address = H160::default();
    let program_seeds = [ &[ACCOUNT_SEED_VERSION], ether_address.as_bytes()];
    let  (new_account, nonce) = Pubkey::find_program_address(&program_seeds, &evm_loader);

    let operator =  Pubkey::new_unique();
    let program_indices = [0, 1];


    println!("new_acc: {}, {}", ether_address, new_account);
    println!("operator: {}", operator);

    let keyed_accounts: Vec<(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)> =  vec![
        (
            false,
            false,
            bpf_loader::id(),
            Rc::new(RefCell::new(bpf_loader_shared()))
        ),
        (
            false,
            false,
            evm_loader,
            Rc::new(RefCell::new(evm_loader_shared()))
        ),
        (
            true,
            true,
            operator,
            AccountSharedData::new_ref(1_000_000_000, 0, &system_program::id()),
        ),
        (
            false,
            false,
            system_program::id(),
            Rc::new(RefCell::new(system_shared()))
        ),
        (
            false,
            true,
            new_account,
            AccountSharedData::new_ref(0, 0, &system_program::id()),
        ),
    ];

    let ix_data: Vec<u8>= serialize(&(24_u8, ether_address.as_fixed_bytes(), nonce)).unwrap();


    vm::run(
        &evm_contract,
        feature_set(),
        keyed_accounts,
        &ix_data,
        &program_indices,
    )
}