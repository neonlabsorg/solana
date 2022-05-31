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
    transaction,
    instruction::{Instruction, AccountMeta},
    message::{
        SanitizedMessage,
        Message,
    },

};
use solana_program_runtime::invoke_context::TransactionAccountRefCell;

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
use std::collections::BTreeMap;


pub fn process(
    opt: &program_options::Opt
) -> Result<(), anyhow::Error> {

    let evm_contract = read_elf::read_so(opt)?;

    let evm_loader_key = Pubkey::from_str(&evm_loader_str)?;

    let ether_address = H160::default();
    let program_seeds = [ &[ACCOUNT_SEED_VERSION], ether_address.as_bytes()];
    let  (new_account_key, nonce) = Pubkey::find_program_address(&program_seeds, &evm_loader_key);

    let operator_key =  Pubkey::new_unique();


    println!("new_acc: {}, {}", ether_address, new_account_key);
    println!("operator: {}", operator_key);

    let mut accounts = BTreeMap::from([
        ( evm_loader_key, Rc::new(RefCell::new(evm_loader_shared())) ),
        ( operator_key, AccountSharedData::new_ref(1_000_000_000, 0, &system_program::id()) ),
        ( system_program::id(), Rc::new(RefCell::new(system_shared())) ),
        ( new_account_key, AccountSharedData::new_ref(0, 0, &system_program::id()) ),
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