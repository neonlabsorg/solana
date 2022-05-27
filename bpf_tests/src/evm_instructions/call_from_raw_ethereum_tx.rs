use crate::evm_instructions::keccak_secp256k1::make_keccak_instruction;
use crate::read_elf;
use crate::program_options;
use crate::vm;
use bincode::serialize;

use evm::{H160, U256};
use evm_loader::account::ACCOUNT_SEED_VERSION;

use solana_program::account_info::AccountInfo;

use solana_sdk::{
    account::{AccountSharedData,  Account},
    // account_info::AccountInfo,
    bpf_loader,
    native_loader,
    system_program,
    sysvar::instructions,
    instruction::{Instruction, AccountMeta},
    message::{
        SanitizedMessage,
        Message,
    }
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
    evm_loader_shared,
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




pub fn read_contract(file_name: &str)->std::io::Result<Vec<u8>> {
    let mut f = File::open(file_name)?;
    let mut bin = vec![];
    f.read_to_end(&mut bin)?;
    Ok(bin)
}



pub fn account_info<'a>(key: &'a Pubkey, account: &'a mut Account) -> AccountInfo<'a> {
    AccountInfo {
        key,
        is_signer: false,
        is_writable: false,
        lamports: Rc::new(RefCell::new(&mut account.lamports)),
        data: Rc::new(RefCell::new(&mut account.data)),
        owner: &account.owner,
        executable: account.executable,
        rent_epoch: account.rent_epoch,
    }
}


pub fn process(
    opt: &program_options::Opt
) -> Result<(), anyhow::Error> {

    let evm_contract = read_elf::read_so(opt)?;

    let evm_loader_key = Pubkey::from_str(&evm_loader_str).unwrap();
    let operator_key= solana_sdk::pubkey::Pubkey::new_from_array(AUTHORIZED_OPERATOR_LIST[0].to_bytes());
    let code_key = Pubkey::new_unique();

    //treasury
    let treasury_index: u32 = 1;
    let seed = format!("{}{}", collateral_pool_base::PREFIX, treasury_index);
    let collateral_pool_base= &solana_sdk::pubkey::Pubkey::new_from_array(collateral_pool_base::id().to_bytes());
    let treasury_key = Pubkey::create_with_seed(&collateral_pool_base, &seed, &evm_loader_key).unwrap();

    //caller
    let caller_address = H160::from_str("0000000000000000000000000000000000000001").unwrap();
    let caller_seeds = [ &[ACCOUNT_SEED_VERSION], caller_address.as_bytes()];
    let  (caller_key, caller_key_nonce) = Pubkey::find_program_address(&caller_seeds, &evm_loader_key);
    let mut caller  = ether_account::Data {
        address : caller_address,
        bump_seed: caller_key_nonce,
        trx_count: 0,
        balance: U256::from(1_000_000_000_u32),
        code_account: None,
        rw_blocked:  false,
        ro_blocked_count: 0,
    };

    let mut caller_shared = AccountSharedData::new(1_000_000_000, ether_account::Data::SIZE+1, &evm_loader_key);
    let (mut tag, mut bytes) = caller_shared.data_mut().split_first_mut().expect("error");
    *tag = 10;
    caller.pack(bytes);

    // contract
    let contract_address = H160::from_str("0000000000000000000000000000000000000002").unwrap();
    let contract_seeds = [ &[ACCOUNT_SEED_VERSION], contract_address.as_bytes()];
    let  (contract_key, contract_key_nonce) = Pubkey::find_program_address(&contract_seeds, &evm_loader_key);

    let contract  = ether_account::Data {
        address : contract_address,
        bump_seed: contract_key_nonce,
        trx_count: 0,
        balance: U256::from(1_000_000_000_u32),
        code_account: Some(evm_loader::solana_program::pubkey::Pubkey::new_from_array(code_key.to_bytes()),),
        rw_blocked:  false,
        ro_blocked_count: 0,
    };
    let mut contract_shared = AccountSharedData::new(1_000_000_000, ether_account::Data::SIZE+1, &evm_loader_key);
    let (mut tag, mut bytes) = contract_shared.data_mut().split_first_mut().expect("error");
    *tag = 10;  // TAG_ACCOUNT
    contract.pack(bytes);


    //code
    let hello_world = read_contract("/home/user/CLionProjects/neonlabs/solana/bpf_tests/contracts/helloWorld.binary").unwrap();
    let code = ether_contract::Data{
        owner: evm_loader::solana_program::pubkey::Pubkey::new_from_array(contract_key.to_bytes()),
        code_size: hello_world.len() as u32
    };
    let mut code_shared = AccountSharedData::new(1_000_000_000_000_000_000, ether_contract::Data::SIZE+1+2048, &evm_loader_key);
    let (mut tag, mut bytes) = code_shared.data_mut().split_first_mut().expect("error");
    let (data, remainig) = bytes.split_at_mut(ether_contract::Data::SIZE);
    code.pack(data);
    remainig[..hello_world.len()].copy_from_slice(hello_world.as_slice());
    *tag = 2;   // TAG_CONTRACT

    // let code_size = code.code_size as usize;
    // let valids_size = (code_size / 8) + 1;


    let token_key = Pubkey::new_from_array(spl_token::id().to_bytes());

    let keccak_key = Pubkey::from_str("KeccakSecp256k11111111111111111111111111111").unwrap();


    let mut keccak_shared = AccountSharedData::new(0, 17, &native_loader::id());
    keccak_shared.set_executable(true);
    let mut data= keccak_shared.data_mut().as_mut_slice();
    data.copy_from_slice(String::from("secp256k1_program").as_bytes());

    let  accounts = BTreeMap::from([
        ( evm_loader_key, Rc::new(RefCell::new(evm_loader_shared())) ),
        (instructions::id(), Rc::new(RefCell::new(sysvar_shared()))),

        (operator_key, AccountSharedData::new_ref(1_000_000_000, 0, &system_program::id())),

        (treasury_key, AccountSharedData::new_ref(0, 0, &evm_loader_key)),

        (caller_key, Rc::new(RefCell::new(caller_shared))),

        (system_program::id(), Rc::new(RefCell::new(system_shared()))),

        (evm_loader_key, Rc::new(RefCell::new(evm_loader_shared()))),

        (contract_key, Rc::new(RefCell::new(contract_shared))),
        (code_key, Rc::new(RefCell::new(code_shared))),

        (token_key, AccountSharedData::new_ref(0, 0, &bpf_loader::id())),
        (keccak_key, Rc::new(RefCell::new(keccak_shared) )),
    ]);

    let meta = vec![
        AccountMeta::new_readonly(instructions::id(), false),
        AccountMeta::new(operator_key, true),
        AccountMeta::new(treasury_key, false),
        AccountMeta::new(caller_key, false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(evm_loader_key, false),
        AccountMeta::new(contract_key, false),
        AccountMeta::new(code_key, false),
        AccountMeta::new_readonly(token_key, false),
    ];

    println!("operator_key {}", operator_key);
    println!("treasure_key {}", treasury_key);
    println!("caller_key {}", caller_key);
    println!("contract_key {}", contract_key);
    println!("code_key {}", code_key);


    let (sig, msg) = make_ethereum_transaction(caller.trx_count, &contract.address);
    let mut ix_data:Vec<u8> = Vec::new();
    ix_data.push(5_u8);
    ix_data.extend_from_slice(&treasury_index.to_le_bytes());
    ix_data.extend_from_slice(caller.address.as_bytes());
    ix_data.extend_from_slice(sig.as_slice());
    ix_data.extend_from_slice(msg.as_slice());


    let instruction_05 = Instruction::new_with_bytes(
        evm_loader_key,
        ix_data.as_slice(),
        meta
    );

    let instruction_keccak = make_keccak_instruction(&contract.address).unwrap();

    let message = SanitizedMessage::Legacy(Message::new(
        &[instruction_keccak, instruction_05 ],
        None,
    ));

    let features =  feature_set();

    vm::run(
        &evm_contract,
        &features,
        &accounts,
        &message,
    )

}