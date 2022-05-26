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


pub fn account_set() -> (Vec<(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)>, Vec<u8>){

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

    // let mut caller_account = Account::new(1_000_000_000, ether_account::Data::SIZE+1, &evm_loader_key);
    // let caller_shared = AccountSharedData::from(caller_account);
    // let caller_info = account_info(&caller_key, &mut caller_account);
    // let a = caller_info.try_borrow_data().unwrap();
    // let caller_shared = AccountSharedData::new_data(caller_info.lamports(), *a, caller_info.owner).unwrap();
    // let a  = evm_loader::solana_program::account_info::AccountInfo::from( caller_info);
    // let init = AccountData::init(&a, caller).unwrap();



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
    remainig.copy_from_slice(hello_world.as_slice());
    *tag = 2;   // TAG_CONTRACT

    let code_size = code.code_size as usize;
    // let valids_size = (code_size / 8) + 1;


    let token_key = Pubkey::new_from_array(spl_token::id().to_bytes());

    let keyed_accounts: Vec<(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)> =  vec![
        (false, false, bpf_loader::id(), Rc::new(RefCell::new(bpf_loader_shared()))),
        (false, false, evm_loader_key, Rc::new(RefCell::new(evm_loader_shared()))),

        (false, false, instructions::id(), Rc::new(RefCell::new(sysvar_shared()))),

        (true, true, operator_key, AccountSharedData::new_ref(1_000_000_000, 0, &system_program::id())),

        (false, true, treasury_key, AccountSharedData::new_ref(0, 0, &evm_loader_key)),

        (false, true, caller_key, Rc::new(RefCell::new(caller_shared))),

        (false, false, system_program::id(), Rc::new(RefCell::new(system_shared()))),

        (false, false, evm_loader_key, Rc::new(RefCell::new(evm_loader_shared()))),

        (false, true, contract_key, Rc::new(RefCell::new(contract_shared))),
        (false, true, code_key, Rc::new(RefCell::new(code_shared))),

        (false, false, token_key, AccountSharedData::new_ref(0, 0, &bpf_loader::id())),
    ];

    println!("operator_key {}", operator_key);
    println!("treasure_key {}", treasury_key);
    println!("caller_key {}", caller_key);
    println!("contract_key {}", contract_key);
    println!("code_key {}", code_key);

    // println!("caller_shared.data: {:?}", cal/ler_shared.data());


    let (sig, msg) = make_ethereum_transaction(caller.trx_count, contract.address);
    let mut ix_data:Vec<u8> = Vec::new();
    ix_data.push(5_u8);
    ix_data.extend_from_slice(&treasury_index.to_le_bytes());
    ix_data.extend_from_slice(caller.address.as_bytes());
    ix_data.extend_from_slice(sig.as_slice());
    ix_data.extend_from_slice(msg.as_slice());

    (keyed_accounts, ix_data)
}


pub fn process(
    opt: &program_options::Opt
) -> Result<(), anyhow::Error> {

    let evm_contract = read_elf::read_so(opt)?;

    let (keyed_accounts, ix_data) = account_set();
    let program_indices = [0, 1];

    vm::run(
        &evm_contract,
        feature_set(),
        keyed_accounts,
        &ix_data,
        &program_indices,
    )
}