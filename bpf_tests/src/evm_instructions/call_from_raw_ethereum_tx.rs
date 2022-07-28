use crate::evm_instructions::{
    feature_set,
    EVM_LOADER_STR,
    make_ethereum_transaction,
    EVM_LOADER_ORIG_STR,
    keccak_secp256k1::make_keccak_instruction
};
use crate::read_elf;
use crate::vm;
use evm_loader::{H160, U256,account::ACCOUNT_SEED_VERSION,
     account::{
         ether_account,
         Packable,
     },
     config::{
         collateral_pool_base,
         AUTHORIZED_OPERATOR_LIST,
     },
};

use solana_sdk::{
    account::AccountSharedData,
    bpf_loader,
    native_loader,
    system_program,
    sysvar::instructions,
    bpf_loader_upgradeable,
    instruction::{Instruction, AccountMeta},
    message::{
        SanitizedMessage,
        Message,
    },
    account::WritableAccount,
};
use solana_program::pubkey::Pubkey;

use std::{
    str::FromStr,
    cell::RefCell,
    rc::Rc,
    collections::BTreeMap,
};
use arrayref::array_mut_ref;


pub fn process() -> Result<(), anyhow::Error> {

    let evm_contract = read_elf::read_so("/home/user/CLionProjects/neonlabs/solana/bpf_tests/contracts/evm_loader.so")?;
    let evm_loader_bin = read_elf::read_bin("/home/user/CLionProjects/neonlabs/solana/bpf_tests/contracts/evm_loader_orig.bin")?;

    let evm_loader_key = Pubkey::from_str(&EVM_LOADER_STR).unwrap();
    let operator_key= Pubkey::new_from_array(AUTHORIZED_OPERATOR_LIST[0].to_bytes());
    let code_key = Pubkey::new_unique();

    //treasury
    let treasury_index: u32 = 1;
    let seed = format!("{}{}", collateral_pool_base::PREFIX, treasury_index);
    let collateral_pool_base= &solana_sdk::pubkey::Pubkey::new_from_array(collateral_pool_base::id().to_bytes());
    let treasury_key = Pubkey::create_with_seed(&collateral_pool_base, &seed, &evm_loader_key).unwrap();

    //caller
    let caller_address = H160::from_str("1000000000000000000000000000000000000001").unwrap();
    let caller_seeds = [ &[ACCOUNT_SEED_VERSION], caller_address.as_bytes()];
    let  (caller_key, caller_key_nonce) = Pubkey::find_program_address(&caller_seeds, &evm_loader_key);
    let caller  = ether_account::Data {
        address : caller_address,
        bump_seed: caller_key_nonce,
        trx_count: 0,
        balance: U256::from(1_000_000_000_u64),
        code_account: None,
        rw_blocked:  false,
        ro_blocked_count: 0,
    };

    let mut caller_shared = AccountSharedData::new(1_000_000_000, ether_account::Data::SIZE+1, &evm_loader_key);
    let (tag, bytes) = caller_shared.data_mut().split_first_mut().expect("error");
    *tag = 10;
    caller.pack(bytes);

    // contract
    let contract_address = H160::from_str("2000000000000000000000000000000000000002").unwrap();
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
    let (tag, bytes) = contract_shared.data_mut().split_first_mut().expect("error");
    *tag = 10;  // TAG_ACCOUNT
    contract.pack(bytes);

    //code
    let mut hello_world_bin = read_elf::read_bin("/home/user/CLionProjects/neonlabs/solana/bpf_tests/contracts/helloWorld.bin")?;

    let owner = array_mut_ref![hello_world_bin, 1, 32];
    owner.copy_from_slice(&contract_key.to_bytes()) ;

    let mut code_shared = AccountSharedData::new(1_000_000_000_000_000_000,   0, &evm_loader_key);
    code_shared.set_data(hello_world_bin);

    let token_key = Pubkey::new_from_array(spl_token::id().to_bytes());
    let keccak_key = Pubkey::from_str("KeccakSecp256k11111111111111111111111111111").unwrap();


    let mut keccak_shared = AccountSharedData::new(0, 17, &native_loader::id());
    keccak_shared.set_executable(true);
    let data= keccak_shared.data_mut().as_mut_slice();
    data.copy_from_slice(String::from("secp256k1_program").as_bytes());

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


    let mut accounts = BTreeMap::from([
        ( evm_loader_key, Rc::new(RefCell::new(evm_loader_shared.clone())) ),

        (operator_key, AccountSharedData::new_ref(1_000_000_000_000_000_000, 0, &system_program::id())),

        (treasury_key, AccountSharedData::new_ref(0, 0, &evm_loader_key)),

        (caller_key, Rc::new(RefCell::new(caller_shared))),

        (system_program::id(), Rc::new(RefCell::new(system_shared))),

        (evm_loader_key, Rc::new(RefCell::new(evm_loader_shared))),

        (contract_key, Rc::new(RefCell::new(contract_shared))),
        (code_key, Rc::new(RefCell::new(code_shared))),

        (token_key, AccountSharedData::new_ref(0, 0, &bpf_loader::id())),
        (keccak_key, Rc::new(RefCell::new(keccak_shared) )),

        (evm_loader_orig_key, Rc::new(RefCell::new(evm_loader_orig_shared))),

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
        AccountMeta::new(caller_key, false),
        AccountMeta::new_readonly(token_key, false),
    ];

    println!("operator_key {}", operator_key);
    println!("treasure_key {}", treasury_key);
    println!("caller_key {}", caller_key);
    println!("contract_key {}", contract_key);
    println!("code_key {}\n\r", code_key);


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
        &mut accounts,
        &message,
    )

}