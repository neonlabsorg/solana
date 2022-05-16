use anyhow::{anyhow};
use std::{
    cell::RefCell,
    io::{self, Write},
    rc::Rc,
    sync::Arc,
    time::Instant,
    borrow::Cow,
};

use solana_bpf_loader_program::{
    create_vm, serialization::serialize_parameters, syscalls::register_syscalls, BpfError,
    ThisInstructionMeter,
};
use solana_program_runtime::{
    compute_budget::ComputeBudget,
    // instruction_processor::{
    //     Executors,
    //     InstructionProcessor,
    // },
    invoke_context::{
        prepare_mock_invoke_context,
        InvokeContext,
        ComputeMeter,
        BuiltinProgram,
        Executors,
    },
    log_collector::LogCollector,
    sysvar_cache::SysvarCache,
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
    // transaction_context::TransactionAccount,
    sysvar,
    slot_hashes::SlotHashes,
};
use solana_sdk::account::ReadableAccount;
use solana_runtime::{builtins, bank::BuiltinPrograms};

// fn fill_sysvar_cache() -> Vec<(Pubkey, Vec<u8>)> {
//     let mut sysvar_cache: Vec<(Pubkey, Vec<u8>)> = vec![];
//     let rent = AccountSharedData::new_data_with_space(1009200, &Rent::default(), 17,  &sysvar::id()).unwrap();
//     sysvar_cache.push((sysvar::rent::id(), rent.data().to_vec()));
//     sysvar_cache
// }

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
    features: FeatureSet,
    keyed_accounts: Vec<(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)>,
    ix_data : &Vec<u8>,
    logs: &Rc<RefCell<LogCollector>>,
    program_indices : &[usize],
)-> Result<u64, anyhow::Error>{

    let config = Config {
        max_call_depth: 100,
        enable_instruction_tracing: false,
        ..Config::default()
    };

    let loader_id = bpf_loader::id();

    let preparation = prepare_mock_invoke_context(program_indices, ix_data, &keyed_accounts);

    let sysvar_cache = fill_sysvar_cache();

    let compute_budget = ComputeBudget {
        max_units: 500_000,
        heap_size: Some(256_usize.saturating_mul(1024)),
        ..ComputeBudget::default()
    };

    let mut builtin_programs: BuiltinPrograms = BuiltinPrograms::default();
    let mut builtins = builtins::get();
    for builtin in builtins.genesis_builtins {
        println!("Adding program {} under {:?}", &builtin.name, &builtin.id);
        builtin_programs.vec.push(BuiltinProgram {
            program_id: builtin.id,
            process_instruction: builtin.process_instruction_with_context,
        });
    };
    // println!("!!!!!!!!!!!   preparation.message.account_keys {:?}", preparation.message.account_keys.len());
    // println!("message.instruction[0].program_id = {:?}", preparation.message.program_id(0));
    // println!("preparation.accounts {:?}", preparation.accounts);
    // println!("preparation.message {:?}", preparation.message);
    // println!("preparation.account_indices {:?}", preparation.account_indices);
    // println!("keyed accounts len = {}", keyed_accounts.len() as u64);
    // println!("preparation accounts len = {}", preparation.accounts.len() as u64);

    let result = {
        let mut invoke_context = InvokeContext::new(
            Rent::default(),
            &preparation.accounts,
            &builtin_programs.vec,
            Cow::Borrowed(&sysvar_cache),
            Some(Rc::clone(&logs)),
            compute_budget,
            Rc::new(RefCell::new(Executors::default())),
            Arc::new(features),
            Hash::default(),
            5_000,
            0,
        );


        invoke_context
            .push(
                &preparation.message,
                &preparation.message.instructions()[0],
                program_indices,
                &preparation.account_indices,
            )
            .unwrap();

        let keyed_accounts = invoke_context.get_keyed_accounts().unwrap();
        // println!("invoke_context.accounts len = {}", keyed_accounts.len().into());

        // let instruction_data = ix_data;
        let (mut parameter_bytes, account_lengths) = serialize_parameters(
            keyed_accounts[0].unsigned_key(),
            keyed_accounts[1].unsigned_key(),
            &keyed_accounts[2..],
            ix_data,
        )
            .unwrap();
        // println!("account_lengths len = {}", account_lengths.len().into());

        let compute_meter = invoke_context.get_compute_meter();
        let mut instruction_meter = ThisInstructionMeter { compute_meter };

        let syscall_registry = register_syscalls(&mut invoke_context).unwrap();
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
            // .unwrap();
        // executable.jit_compile().unwrap();
        Executable::<BpfError, ThisInstructionMeter>::jit_compile(&mut executable).unwrap();

        let mut vm = create_vm(
            &executable,
            parameter_bytes.as_slice_mut(),
            &mut invoke_context,
            &account_lengths,
        ).unwrap();
        let start_time = Instant::now();
        let result = vm.execute_program_jit(&mut instruction_meter);
        let instruction_count = vm.get_total_instruction_count();
        println!(
            "Executed {}  instructions in {:.2}s.",
            // path.to_string_lossy(),
            instruction_count,
            start_time.elapsed().as_secs_f64()
        );

        result
    };
    result.map_err(|e| {anyhow!("exit code: {}", e)})
}

pub fn run(
    contract: &Vec<u8>,
    features: FeatureSet,
    keyed_accounts: Vec<(bool, bool, Pubkey, Rc<RefCell<AccountSharedData>>)>,
    ix_data : &Vec<u8>,
    program_indices : &[usize],

) -> Result<(), anyhow::Error> {
    // let data = read_elf::read_so(&opt)?;

    // let logs = LogCollector::new_ref_with_limit(None);
    let logs = Rc::new(RefCell::new(LogCollector::default()));

    let result = execute(
        contract,
        features,
        keyed_accounts,
        ix_data,
        &logs,
        program_indices
    );

    println!("");
    if let Ok(logs) = Rc::try_unwrap(logs) {
        for message in Vec::from(logs.into_inner()) {
            // println!("{}", message);
            let _ = io::stdout().write_all(message.replace("Program log: ", "Program log: ").as_bytes());
            println!("");
        }
    }

    match result {
        Ok(exit_code) => {
            if exit_code == SUCCESS {
                Ok(())
            } else {
                Err(anyhow!("exit code: {}", exit_code))
            }
        }
        Err(e) => {
            // if false {
            //     let trace = File::create("trace.out").unwrap();
            //     let mut trace = BufWriter::new(trace);
            //     let analysis =
            //         solana_rbpf::static_analysis::Analysis::from_executable(executable.as_ref());
            //     vm.get_tracer().write(&mut trace, &analysis).unwrap();
            // }
            Err(e.into())
        }
    }
}




