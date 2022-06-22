use crate::read_elf;
use crate::vm;
use bincode::serialize;

use evm::{H160, U256};
use evm_loader::account::ACCOUNT_SEED_VERSION;

use solana_program::account_info::AccountInfo;

use solana_sdk::{
    account::{AccountSharedData,  Account},
    instruction::{Instruction, AccountMeta},
    // account_info::AccountInfo,
    bpf_loader,
    native_loader,
    system_program,
    sysvar::instructions,
};

use solana_program:: {
    pubkey::Pubkey,
    keccak::hash,
};

use std::{
    str::FromStr,
    cell::RefCell,
    rc::Rc,
    fs::File,
    io::prelude::*,
    collections::BTreeMap,
};

use solana_sdk::account::{WritableAccount, ReadableAccount};
use hex;

use crate::evm_instructions::{
    feature_set,
    bpf_loader_shared,
    system_shared,
    evm_loader_str,
    sysvar_shared,
    make_ethereum_transaction,
};

use evm_loader::{
    account::{
        ether_account,
        ether_contract,
        Packable,
        AccountData,
    },
    config::{
        collateral_pool_base,
        CHAIN_ID,
        AUTHORIZED_OPERATOR_LIST,
    },

};

use libsecp256k1::{SecretKey, Signature};
use libsecp256k1::PublicKey;

use rlp::RlpStream;
use std::borrow::Borrow;
use std::ops::{Deref, DerefMut};
use std::cell::RefMut;


fn make_keccak_instruction_data(instruction_index : u8, msg_len: u16, data_start : u16) ->Vec<u8> {
    let mut data = Vec::new();

    let check_count : u8 = 1;
    let eth_address_size : u16 = 20;
    let signature_size : u16 = 65;
    let eth_address_offset: u16 = data_start;
    let signature_offset : u16 = eth_address_offset + eth_address_size;
    let message_data_offset : u16 = signature_offset + signature_size;

    data.push(check_count);

    data.push(signature_offset as u8);
    data.push((signature_offset >> 8) as u8);

    data.push(instruction_index);

    data.push(eth_address_offset as u8);
    data.push((eth_address_offset >> 8) as u8);

    data.push(instruction_index);

    data.push(message_data_offset as u8);
    data.push((message_data_offset >> 8) as u8);

    data.push(msg_len as u8);
    data.push((msg_len >> 8) as u8);

    data.push(instruction_index);
    return data;
}


pub fn make_keccak_instruction(contract_address : &H160,) -> Result<(Instruction), anyhow::Error> {

    let keccakprog = Pubkey::from_str("KeccakSecp256k11111111111111111111111111111").unwrap();

    let meta = vec![
        AccountMeta::new(keccakprog, false),
    ];
    let (_sig, msg) = make_ethereum_transaction(0, contract_address);

    let ix_data = make_keccak_instruction_data(1, msg.len() as u16, 5);


    let instruction = Instruction::new_with_bytes(
        keccakprog,
        ix_data.as_slice(),
        meta
    );

    Ok(instruction)
}