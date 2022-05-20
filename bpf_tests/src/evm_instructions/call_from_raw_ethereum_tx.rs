use crate::read_elf;
use crate::program_options;
use crate::vm;
use bincode::serialize;

use evm::{H160, U256};
use evm_loader::account::ACCOUNT_SEED_VERSION;

use solana_sdk::{
    account::AccountSharedData,
    bpf_loader,
    native_loader,
    system_program,
    sysvar::instructions,
};

use solana_program::{
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

use solana_sdk::account::WritableAccount;
use hex;

use crate::evm_instructions::{
    feature_set,
    bpf_loader_shared,
    evm_loader_shared,
    system_shared,
    evm_loader_str,
    sysvar_shared,
};

use evm_loader::{
    account::{
        ether_account,
        ether_contract,
        Packable
    },
    config::{
        collateral_pool_base,
        CHAIN_ID,
    },

};

use libsecp256k1::{SecretKey, Signature};
use libsecp256k1::PublicKey;
use rlp::RlpStream;



pub fn read_contract(file_name: &str)->std::io::Result<Vec<u8>> {
    let mut f = File::open(file_name)?;
    let mut bin = vec![];
    f.read_to_end(&mut bin)?;
    Ok(bin)
}

struct UnsignedTransaction {
    nonce: u64,
    gas_price: U256,
    gas_limit: U256,
    to: Option<H160>,
    value: U256,
    data: Vec<u8>,
    chain_id: U256,
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    hash(data).to_bytes()
}

pub fn make_ethereum_transaction(
    trx_count: u64,
    to: H160,
) -> (Vec<u8>, Vec<u8>) {

    let pk_hex: &[u8] = "0510266f7d37f0957564e4ce1a1dcc8bb3408383634774a2f4a94a35f4bc53e0".as_bytes();
    let pk = SecretKey::from_slice(&hex::decode(pk_hex).unwrap());

    let rlp_data = {
        let tx = UnsignedTransaction {
            to: Some(to),
            nonce: trx_count,
            gas_limit: 9_999_999_999_u64.into(),
            gas_price: 10_u64.pow(9).into(),
            value: 0.into(),
            data: vec![],
            chain_id: CHAIN_ID.into(),
        };

        rlp::encode(&tx).to_vec()
    };

    let (r_s, v) = {
        let msg = libsecp256k1::Message::parse(&keccak256(rlp_data.as_slice()));
        libsecp256k1::sign(&msg, &pk)
    };

    let mut signature : Vec<u8> = Vec::new();
    signature.extend(r_s.serialize().iter().copied());
    signature.push(v.serialize());

    (signature, rlp_data)
}


pub fn account_set() -> (Vec<(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)>, Vec<u8>){

    let evm_loader_key = Pubkey::from_str(&evm_loader_str).unwrap();
    let operator_key = Pubkey::from_str("NeonPQFrw5stVvs1rFLDxALWUBDCnSPsWBP83RfNUKK")?;
    let code_key = Pubkey::new_unique();

    //treasury
    let treasury_index: u32 = 1;
    let seed = format!("{}{}", collateral_pool_base::PREFIX, treasury_index);
    let treasury_key = Pubkey::create_with_seed(&collateral_pool_base::id(), &seed, &evm_loader_key)?;

    //caller
    let user_address = H160::from_str("0000000000000000000000000000000000000001").unwrap();
    let user_seeds = [ &[ACCOUNT_SEED_VERSION], user_address.as_bytes()];
    let  (user_key, user_key_nonce) = Pubkey::find_program_address(&user_seeds, &evm_loader_key);
    let user  = ether_account::Data {
        address : user_address,
        bump_seed: user_key_nonce,
        trx_count: 0,
        balance: U256::from(1_000_000_000_u32),
        code_account: None,
        rw_blocked:  false,
        ro_blocked_count: 0,
    };
    let mut user_data = Vec::with_capacity(ether_account::Data::SIZE+1);
    user_data.resize(user_data.capacity(), 0);
    user.pack(user_data.as_mut_slice());

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
    let mut contract_data = Vec::with_capacity(ether_account::Data::SIZE+1);
    contract_data.resize(contract_data.capacity(), 0);
    contract.pack(contract_data.as_mut_slice());


    //code
    let bin = read_contract("/home/user/CLionProjects/neonlabs/solana/bpf_tests/contracts/helloWorld.binary").unwrap();
    let code = ether_contract::Data{
        owner: evm_loader::solana_program::pubkey::Pubkey::new_from_array(contract_key.to_bytes()),
        code_size: bin.len() as u32
    };
    let mut code_data = Vec::with_capacity(ether_contract::Data::SIZE + 1 + 2048);
    code_data.resize(code_data.capacity(), 0);
    code.pack(code_data.as_mut_slice());

    let token_key = Pubkey::new_from_array(spl_token::id().to_bytes());

    let keyed_accounts: Vec<(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)> =  vec![
        (false, false, bpf_loader::id(), Rc::new(RefCell::new(bpf_loader_shared()))),
        (false, false, evm_loader_key, Rc::new(RefCell::new(evm_loader_shared()))),

        (false, false, instructions::id(), Rc::new(RefCell::new(sysvar_shared()))),

        (true, true, operator_key, AccountSharedData::new_ref(1_000_000_000, 0, &system_program::id())),

        (false, true, treasury_key, AccountSharedData::new_ref(0, 0, &evm_loader_key)),

        (false, true, user_key, Rc::new(AccountSharedData::new_ref_data(0, &user_data, &evm_loader_key).unwrap())),

        (false, false, system_program::id(), Rc::new(RefCell::new(system_shared()))),

        (false, false, evm_loader_key, Rc::new(RefCell::new(evm_loader_shared()))),

        (false, true, contract_key, Rc::new(AccountSharedData::new_ref_data(0, &contract_data, &evm_loader_key).unwrap())),
        (false, true, code_key, Rc::new(AccountSharedData::new_ref_data(0, &code_data, &evm_loader_key).unwrap())),

        (false, false, token_key, AccountSharedData::new_ref(0, 0, &bpf_loader::id())),
    ];

    println!("operator_key {}", operator_key);
    println!("treasure_key {}", treasure_key);
    println!("user_key {}", user_key);
    println!("contract_key {}", contract_key);
    println!("code_key {}", code_key);

    let (sig, msg) = make_ethereum_transaction(user.trx_count, contract.address);
    let mut ix_data:Vec<u8> = Vec::new();
    ix_data.push(5_u8);
    ix_data.extend_from_slice(&treasury_index.to_le_bytes());
    ix_data.extend_from_slice(user.address.as_bytes());
    ix_data.extend_from_slice(sig.as_slice());
    ix_data.extend_from_slice(msg.as_slice());

    (keyed_accounts, ix_data)
}


pub fn process(
    opt: &program_options::Opt
) -> Result<(), anyhow::Error> {

    let evm_contract = read_elf::read_so(opt)?;

    let (keyed_accounts, ix_data) = account_set();

    vm::run(
        &evm_contract,
        feature_set(),
        keyed_accounts,
        &ix_data,
        &program_indices,
    )
}