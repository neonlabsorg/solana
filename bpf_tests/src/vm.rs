use crate::evm_instructions::{EVM_LOADER_STR,  EVM_LOADER_ORIG_STR};

use anyhow::{anyhow};
use std::{
    cell::RefCell,
    io::{self, Write},
    rc::Rc,
    sync::Arc,
    time::Instant,
    borrow::Cow,
    str::FromStr,
    collections::BTreeMap,
};

use solana_bpf_loader_program::{
    create_vm, serialization::serialize_parameters, syscalls::register_syscalls, BpfError,
    ThisInstructionMeter,
    serialization::deserialize_parameters,
};
use solana_program_runtime::{
    compute_budget::ComputeBudget,
    invoke_context::{
        InvokeContext,
        BuiltinProgram,
        Executors,
        TransactionAccountRefCell,
    },
    log_collector::LogCollector,
    sysvar_cache::SysvarCache,
    stable_log,
};
use solana_rbpf::{elf::Executable, vm::Config};
#[allow(deprecated)]
#[allow(unused_imports)]
use solana_sdk::{
    account::{AccountSharedData, Account},  entrypoint::SUCCESS,
    feature_set::{FeatureSet, instructions_sysvar_owned_by_sysvar},
    hash::Hash,
    pubkey::Pubkey,
    rent::Rent,
    clock::Clock,
    sysvar::fees::Fees,
    epoch_schedule::EpochSchedule,
    sysvar,
    slot_hashes::SlotHashes,
    message::{
        SanitizedMessage,
    },
    secp256k1_program,
    sysvar::{
        instructions::{
            construct_instructions_data},
    },
    system_program,
};

use solana_runtime::{
    builtins,
    bank::BuiltinPrograms,
};

use solana_sdk::{
    feature_set::do_support_realloc,
    transaction::{TransactionError},
    precompiles::verify_if_precompile,
    sysvar::instructions,
    account::WritableAccount,
};


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
        max_units: 1_000_000_000_000_000,
        heap_size: Some(256_usize.saturating_mul(1024)),
        ..ComputeBudget::default()
    };

    let mut builtin_programs: BuiltinPrograms = BuiltinPrograms::default();
    let builtins = builtins::get();
    for builtin in builtins.genesis_builtins {
        builtin_programs.vec.push(BuiltinProgram {
            program_id: builtin.id,
            process_instruction: builtin.process_instruction_with_context,
        });
    };
    let bpf_loader = solana_bpf_loader_program::solana_bpf_loader_program!();
    let upgradable_loader = solana_bpf_loader_program::solana_bpf_loader_upgradeable_program!();

    builtin_programs.vec.push(BuiltinProgram {
        program_id: solana_sdk::bpf_loader::id(),
        process_instruction: bpf_loader.2,
    });

    builtin_programs.vec.push(BuiltinProgram {
        program_id: solana_sdk::bpf_loader_upgradeable::id(),
        process_instruction: upgradable_loader.2,
    });


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

    for (pubkey, account) in accounts_ordered.iter().take(message.account_keys_len()) {
        if instructions::check_id(pubkey) {
            let mut mut_account_ref = account.borrow_mut();
            instructions::store_current_index(
                mut_account_ref.data_as_mut_slice(),
                instruction_index as u16,
            );
            break;
        }
    }


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
                println! ("error {:?}", e);
                return Err(anyhow!("error {:?}", e));
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
    let units_before = compute_meter.try_borrow().unwrap().get_remaining();

    let result = vm.execute_program_jit(&mut instruction_meter);

    let units_after = compute_meter.try_borrow().unwrap().get_remaining();
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
    ).unwrap();


    let  return_data = &invoke_context.return_data.1;

    if !return_data.is_empty() {
        stable_log::program_return(&log_collector, &program_id, &return_data);
    }
    else{
        stable_log::program_return(&log_collector, &program_id, &vec![]);
    }


    println!("Executed {}  instructions in {:.2}s.", instruction_count, start_time.elapsed().as_secs_f64());
    println!("Program  {} consumed {} of {} compute units", program_id, units_before - units_after, units_before);

    result.map_err(|e| {anyhow!("exit code: {}", e)})
}


/// Verify the precompiled programs in this transaction.
#[allow(unused)]
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

fn construct_instructions_account(
    message: &SanitizedMessage,
    is_owned_by_sysvar: bool,
) -> AccountSharedData {
    let data = construct_instructions_data(&message.decompile_instructions());
    let owner = if is_owned_by_sysvar {
        sysvar::id()
    } else {
        system_program::id()
    };
    AccountSharedData::from(Account {
        data,
        owner,
        ..Account::default()
    })
}


pub fn run(
    contract: &Vec<u8>,
    features: &Arc<FeatureSet>,
    accounts: &mut BTreeMap<Pubkey, Rc<RefCell<AccountSharedData>>>,
    message: &SanitizedMessage,
) -> Result<(), anyhow::Error> {

    let logs = Rc::new(RefCell::new(LogCollector::default()));

    // secp256k1_program
    // verify_precompiles(message, features).map_err(|e| anyhow!("precompile instruction error: {:?}", e )).unwrap();

    let is_active = features.is_active(&instructions_sysvar_owned_by_sysvar::id());

    for  key in message.account_keys_iter() {
        if solana_sdk::sysvar::instructions::check_id(key) {
            let sysvar_shared = construct_instructions_account(
                message,
                is_active
            );
            accounts.insert(
                sysvar::instructions::id(), Rc::new(RefCell::new(sysvar_shared))
            );
        }
    }

    let mut accounts_ordered :Vec<TransactionAccountRefCell> = Vec::new();

    for key in message.account_keys_iter() {
        let value : TransactionAccountRefCell = (*key, accounts.get(key).unwrap().clone() );
        accounts_ordered.push(value );
    }

    let evm_loader_orig_key = solana_sdk::pubkey::Pubkey::from_str(EVM_LOADER_ORIG_STR).unwrap();
    let value : TransactionAccountRefCell = (evm_loader_orig_key, accounts.get(&evm_loader_orig_key).unwrap().clone() );
    accounts_ordered.push(value );


    let evm_loader_key = Pubkey::from_str(&EVM_LOADER_STR)?;

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

    println!("");
    if let Ok(logs) = Rc::try_unwrap(logs) {
        for message in Vec::from(logs.into_inner()) {
            // println!("{}", message);
            let _ = io::stdout().write_all(message.replace("Program log: ", "Program log: ").as_bytes());
            println!("");
        }
    }
    Ok(())
}





