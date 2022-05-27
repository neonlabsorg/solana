use crate::evm_instructions::{
    evm_loader_str,
    evm_loader_shared,
};

use anyhow::{anyhow};
use std::{
    cell::RefCell,
    io::{self, Write},
    rc::Rc,
    sync::Arc,
    time::Instant,
    borrow::Cow,
    str::FromStr,
};

use solana_bpf_loader_program::{
    create_vm, serialization::serialize_parameters, syscalls::register_syscalls, BpfError,
    ThisInstructionMeter,
    serialization::deserialize_parameters,
};
use solana_program_runtime::{
    compute_budget::ComputeBudget,
    invoke_context::{
        prepare_mock_invoke_context,
        InvokeContext,
        ComputeMeter,
        BuiltinProgram,
        Executors,
        TransactionAccountRefCell,
    },
    log_collector::LogCollector,
    sysvar_cache::SysvarCache,
    stable_log,
    timings::ExecuteTimings,
};
use solana_rbpf::{elf::Executable, vm::Config};
use solana_sdk::{
    account::AccountSharedData, bpf_loader, entrypoint::SUCCESS,
    feature_set::FeatureSet,
    hash::Hash,
    pubkey::Pubkey,
    rent::Rent,
    clock::Clock,
    sysvar::fees::Fees,
    epoch_schedule::EpochSchedule,
    sysvar,
    slot_hashes::SlotHashes,
    keyed_account::keyed_account_at_index,
    native_loader,
    message::{
        SanitizedMessage,
        Message,
    },
    secp256k1_program,
    secp256k1_instruction::new_secp256k1_instruction,
    instruction::InstructionError,
    instruction::Instruction
};

use solana_runtime::{
    builtins,
    bank::BuiltinPrograms,
    message_processor::MessageProcessor,

};
use std::borrow::Borrow;
use std::{ fmt::Debug, pin::Pin};


use solana_sdk::{
    feature_set::do_support_realloc,
    transaction::{TransactionError},
    precompiles::verify_if_precompile,
};
use std::fs::File;
use std::io::BufWriter;
use std::ops::{Deref, Index};

use libsecp256k1::{SecretKey, Signature};
use libsecp256k1::PublicKey;
use solana_sdk::account::{WritableAccount, ReadableAccount};
use std::collections::BTreeMap;

fn fill_sysvar_cache() -> SysvarCache {
    let mut sysvar_cache =  SysvarCache::default();

    if sysvar_cache.get_clock().is_err() {
        sysvar_cache.set_clock(Clock::default());
    }

    if sysvar_cache.get_epoch_schedule().is_err() {
        sysvar_cache.set_epoch_schedule(EpochSchedule::default());
    }

    #[allow(deprecated)]
    if sysvar_cache.get_fees().is_err() {
        sysvar_cache.set_fees(Fees::default());
    }

    if sysvar_cache.get_rent().is_err() {
        sysvar_cache.set_rent(Rent::default());
    }

    if sysvar_cache.get_slot_hashes().is_err() {
        sysvar_cache.set_slot_hashes(SlotHashes::default());
    }
    sysvar_cache
}


fn execute(
    contract: &Vec<u8>,
    features: Arc<FeatureSet>,
    accounts_ordered: &Vec<TransactionAccountRefCell>,
    logs: &Rc<RefCell<LogCollector>>,
    program_index :usize,
    instruction_index :usize,
    message :&SanitizedMessage,
)-> Result<u64, anyhow::Error>{

    let config = Config {
        max_call_depth: 100,
        enable_instruction_tracing: false,
        ..Config::default()
    };

    let program_id = &accounts_ordered[program_index].0;

    let sysvar_cache = fill_sysvar_cache();

    let compute_budget = ComputeBudget {
        max_units: 500_000,
        heap_size: Some(256_usize.saturating_mul(1024)),
        ..ComputeBudget::default()
    };

    let mut builtin_programs: BuiltinPrograms = BuiltinPrograms::default();
    let mut builtins = builtins::get();
    for builtin in builtins.genesis_builtins {
        // println!("Adding program {} under {:?}", &builtin.name, &builtin.id);
        builtin_programs.vec.push(BuiltinProgram {
            program_id: builtin.id,
            process_instruction: builtin.process_instruction_with_context,
        });
    };


    let mut invoke_context = InvokeContext::new(
        Rent::default(),
        &accounts_ordered.as_slice(),
        &builtin_programs.vec,
        Cow::Borrowed(&sysvar_cache),
        Some(Rc::clone(&logs)),
        compute_budget,
        Rc::new(RefCell::new(Executors::default())),
        features,
        Hash::default(),
        5_000,
        0,
    );


    invoke_context
        .push(
            &message,
            &message.instructions()[instruction_index],
            &vec![program_index],
            &[],
        )
        .unwrap();

    let stack_height = invoke_context.get_stack_height();
    let log_collector = invoke_context.get_log_collector();
    let compute_meter = invoke_context.get_compute_meter();
    let mut instruction_meter = ThisInstructionMeter {compute_meter: compute_meter.clone() };

    let invoke_context_mut = &mut invoke_context;

    let  keyed_accounts = invoke_context_mut.get_keyed_accounts().unwrap();
    let (mut parameter_bytes, account_lengths) = serialize_parameters(
        &keyed_accounts[0].owner().unwrap(),
        keyed_accounts[0].unsigned_key(),
        &keyed_accounts[1..],
        message.instructions()[instruction_index].data.as_slice(),
    )
        .unwrap();

    let syscall_registry = register_syscalls(invoke_context_mut).unwrap();
    let mut executable =
        match Executable::<BpfError, ThisInstructionMeter>::from_elf(
            contract,
            None,
            config,
            syscall_registry,
        ){
            Ok(a) => a,
            Err(e) => {
                println! ("error {}", e);
                return Err(anyhow!(""));
            }
        };
    Executable::<BpfError, ThisInstructionMeter>::jit_compile(&mut executable).unwrap();

    let mut vm = create_vm(
        &executable,
        parameter_bytes.as_slice_mut(),
        invoke_context_mut,
        &account_lengths,
    ).unwrap();

    stable_log::program_invoke(&log_collector, &program_id, stack_height);

    let start_time = Instant::now();
    let before: u64 = compute_meter.try_borrow().unwrap().get_remaining();

    let result = vm.execute_program_jit(&mut instruction_meter);

    let after:u64 = compute_meter.try_borrow().unwrap().get_remaining();
    let instruction_count = vm.get_total_instruction_count();

    drop(vm);

    let keyed_accounts =  invoke_context_mut.get_keyed_accounts().unwrap();
    deserialize_parameters(
        &keyed_accounts[0].owner().unwrap(),
        &keyed_accounts[1..],
        parameter_bytes.as_slice(),
        &account_lengths,
        invoke_context_mut
            .feature_set
            .is_active(&do_support_realloc::id()),
    );

    println!("Executed {}  instructions in {:.2}s.", instruction_count, start_time.elapsed().as_secs_f64());
    println!("Program  {} consumed {} of {} compute units", program_id, before - after, before);

    let  return_data = &invoke_context.return_data.1;

    if !return_data.is_empty() {
        stable_log::program_return(&log_collector, &program_id, &return_data);
    }
    else{
        stable_log::program_return(&log_collector, &program_id, &vec![]);
    }


    result.map_err(|e| {anyhow!("exit code: {}", e)})
}


/// Verify the precompiled programs in this transaction.
pub fn verify_precompiles(message: &SanitizedMessage, feature_set: &Arc<FeatureSet>) -> Result<(), TransactionError> {
    for instruction in message.instructions() {
        // The Transaction may not be sanitized at this point
        if instruction.program_id_index as usize >= message.account_keys_len() {
            return Err(TransactionError::AccountNotFound);
        }
        let program_id = &message.account_keys_iter().nth(instruction.program_id_index as usize).unwrap();

        verify_if_precompile(
            program_id,
            instruction,
            &message.instructions(),
            feature_set,
        )
            .map_err(|_| TransactionError::InvalidAccountIndex)?;
    }
    Ok(())
}


pub fn run(
    contract: &Vec<u8>,
    features: &Arc<FeatureSet>,
    accounts: &BTreeMap<Pubkey, Rc<RefCell<AccountSharedData>>>,
    message: &SanitizedMessage,
) -> Result<(), anyhow::Error> {

    // secp256k1_program
    verify_precompiles(message, features).map_err(|e| anyhow!("precompile instruction error: {:?}", e )).unwrap();

    println!("verify_precompiles is completed");

    let logs = Rc::new(RefCell::new(LogCollector::default()));

    let mut accounts_ordered :Vec<TransactionAccountRefCell> = Vec::new();

    for key in message.account_keys_iter() {
        // println!("key {}", key);
        let value : TransactionAccountRefCell = (*key, accounts.get(key).unwrap().clone() );
        accounts_ordered.push(value );
    }
    let evm_loader_key = Pubkey::from_str(&evm_loader_str)?;

    let program_index = accounts_ordered.iter().position(|item|item.0 == evm_loader_key ).unwrap();

    for instruction_index in 0..message.instructions().len(){
        let program_id = message.get_account_key(message.instructions()[instruction_index].program_id_index as usize).unwrap();
        // execute only evm_loader instructions
        if *program_id != evm_loader_key{
            continue;
        };

        let result = execute(
            contract,
            features.clone(),
            &accounts_ordered,
            &logs,
            program_index,
            instruction_index,
            message,
        );

        match result {
            Ok(exit_code) => {
                if exit_code != SUCCESS {
                    println!("exit code: {}", exit_code)
                }
            }
            Err(e) => {
                println!("error: {:?}", e)
            }
         }
    }

    println!("logs:");
    if let Ok(logs) = Rc::try_unwrap(logs) {
        for message in Vec::from(logs.into_inner()) {
            // println!("{}", message);
            let _ = io::stdout().write_all(message.replace("Program log: ", "Program log: ").as_bytes());
            println!("");
        }
    }

    for i in accounts_ordered{
        println!("{:?}", i);
    }

    Ok(())
}





