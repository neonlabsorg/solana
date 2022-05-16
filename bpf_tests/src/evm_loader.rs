use evm_loader::account::EthereumAccount;
use crate::read_elf;
use crate::program_options;
use crate::vm_1_9_12;
use bincode::serialize;

use evm::{H160, };


use solana_sdk::{
    feature_set::{
        FeatureSet,
        tx_wide_compute_cap,
        requestable_heap_size,
        remove_native_loader,
        // demote_program_write_locks,
    },
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

pub const ACCOUNT_SEED_VERSION: u8 = 1_u8;



pub fn create_account(
    opt: &program_options::Opt
) -> Result<(), anyhow::Error> {

    let contract = read_elf::read_so(opt)?;

    let mut features = FeatureSet::all_enabled();
    features.deactivate(&tx_wide_compute_cap::id());
    features.deactivate(&requestable_heap_size ::id());
    // features.deactivate(&remove_native_loader ::id());
    // features.deactivate(&demote_program_write_locks::id());



    let evm_loader = Pubkey::from_str("eeLSJgWzzxrqKv1UxtRVVH8FX3qCQWUs9QuAjJpETGU")?;

    let ether_address = H160::default();
    let program_seeds = [ &[ACCOUNT_SEED_VERSION], ether_address.as_bytes()];
    let  (new_account, nonce) = Pubkey::find_program_address(&program_seeds, &evm_loader);

    let operator =  Pubkey::new_unique();


    let mut bpf_loader_sh = AccountSharedData::new(1_000_000_000_000_000_000, 25, &native_loader::id());
    bpf_loader_sh.set_executable(true);

    let mut evm_loader_sh = AccountSharedData::new(1_000_000_000_000_000_000, 36, &bpf_loader::id());
    evm_loader_sh.set_executable(true);


    let mut system_sh = AccountSharedData::new(1_000_000_000, 14, &native_loader::id());
    system_sh.set_executable(true);

    let program_indices = [0, 1];


    println!("program_seeds: {:?}", &program_seeds.to_vec());
    println!("new_acc: {}, {}", ether_address, new_account);
    println!("operator: {}", operator);

    let keyed_accounts: Vec<(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)> =  vec![
        (
            false,
            false,
            bpf_loader::id(),
            Rc::new(RefCell::new(bpf_loader_sh))
        ),
        (
            false,
            false,
            evm_loader,
            Rc::new(RefCell::new(evm_loader_sh))
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
            Rc::new(RefCell::new(system_sh))
        ),
        (
            false,
            true,
            new_account,
            AccountSharedData::new_ref(0, 0, &system_program::id()),
        ),
        // (
        //     false,
        //     true,
        //     new_account,
        //     AccountSharedData::new_ref(1_000_000_000, EthereumAccount::SIZE, &system_program::id()),
        // ),
    ];

    let ix_data: Vec<u8>= serialize(&(24_u8, ether_address.as_fixed_bytes(), nonce)).unwrap();


    vm_1_9_12::run(
        &contract,
        features,
        keyed_accounts,
        &ix_data,
        &program_indices,
    )
}