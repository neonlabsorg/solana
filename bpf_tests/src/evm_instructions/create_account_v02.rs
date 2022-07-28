use crate::evm_instructions::{
    feature_set,
    EVM_LOADER_STR,
    EVM_LOADER_ORIG_STR,
};
use crate::read_elf;
use crate::vm;
use evm_loader::{account::ACCOUNT_SEED_VERSION, H160};

use solana_sdk::{
    account::AccountSharedData,
    native_loader,
    pubkey::Pubkey,
    system_program,
    instruction::{Instruction, AccountMeta},
    message::{
        SanitizedMessage,
        Message,
    },
    bpf_loader_upgradeable,
    account::WritableAccount,
};

use std::{
    str::FromStr,
    cell::RefCell,
    rc::Rc,
    collections::BTreeMap,
};
use bincode::serialize;


#[allow(unused)]
pub fn process() -> Result<(), anyhow::Error> {
    let evm_contract = read_elf::read_so("/home/user/CLionProjects/neonlabs/solana/bpf_tests/contracts/evm_loader.so")?;
    let evm_loader_bin = read_elf::read_bin("/home/user/CLionProjects/neonlabs/solana/bpf_tests/contracts/evm_loader_orig.bin")?;

    let evm_loader_key = Pubkey::from_str(&EVM_LOADER_STR)?;

    let ether_address = H160::default();
    let program_seeds = [ &[ACCOUNT_SEED_VERSION], ether_address.as_bytes()];
    let  (new_account_key, nonce) = Pubkey::find_program_address(&program_seeds, &evm_loader_key);

    let operator_key =  Pubkey::new_unique();


    let evm_loader_orig_key = solana_sdk::pubkey::Pubkey::from_str(EVM_LOADER_ORIG_STR).unwrap();
    let mut evm_loader_orig_shared = AccountSharedData::new(25_000_000_000, evm_loader_bin.len(), &bpf_loader_upgradeable::id());
    let data= evm_loader_orig_shared.data_mut().as_mut_slice();
    data.copy_from_slice(evm_loader_bin.as_slice());


    let mut evm_loader_shared = AccountSharedData::new(1_000_000_000_000_000_000, 36, &bpf_loader_upgradeable::id());
    evm_loader_shared.set_executable(true);
    let data= evm_loader_shared.data_mut().as_mut_slice();

    data[..4].copy_from_slice(vec![2, 0, 0, 0].as_slice());
    data[4..].copy_from_slice(evm_loader_orig_key.to_bytes().as_slice());

    let mut system_shared = AccountSharedData::new(1_000_000_000, 14, &native_loader::id());
    system_shared.set_executable(true);

    println!("new_acc: {}, {}", ether_address, new_account_key);
    println!("operator: {}", operator_key);

    let mut accounts = BTreeMap::from([
        ( evm_loader_key, Rc::new(RefCell::new(evm_loader_shared)) ),
        ( operator_key, AccountSharedData::new_ref(1_000_000_000, 0, &system_program::id()) ),
        ( system_program::id(), Rc::new(RefCell::new(system_shared)) ),
        ( new_account_key, AccountSharedData::new_ref(0, 0, &system_program::id()) ),
        ( evm_loader_orig_key, Rc::new(RefCell::new(evm_loader_orig_shared))),
    ]);

    let ix_data: Vec<u8>= serialize(&(24_u8, ether_address.as_fixed_bytes(), nonce)).unwrap();


    let meta = vec![
        AccountMeta::new(operator_key, true),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new(new_account_key, false),
    ];

    let instruction = Instruction::new_with_bytes(
        evm_loader_key,
        ix_data.as_slice(),
        meta
    );

    let message = SanitizedMessage::Legacy(Message::new(
        &[instruction],
        None,
    ));

    let features =  feature_set();

    vm::run(
        &evm_contract,
        &features,
        &mut accounts,
        &message,
    )
}