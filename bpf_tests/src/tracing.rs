use evm::{Event, *};
use evm::{U256, Memory, Stack, Context, Transfer, ExitReason, Capture, Trap};
use solana_bpf_loader_program::syscalls as syscalls;
use solana_rbpf::memory_region::MemoryMapping;
use solana_sdk::pubkey::Pubkey;
use std::{
    slice::from_raw_parts,
    mem::size_of,
};
use std::ops::Deref;
use std::borrow::BorrowMut;


pub struct Tracer{
    // events: Vec<Event<'a>>,
    pub remaining: u64,
}

impl Tracer  {
    pub fn new() -> Self {
        Tracer {
            // events: vec![],
            remaining: 0,
        }
    }
}

pub fn read_vec<T>(vm_addr: &Vec<T>, memory_mapping: &MemoryMapping, loader_id: &Pubkey) -> Vec<T>
    where
        T: std::clone::Clone
{
    let vec = &syscalls::translate_slice::<Vec<T>>(
        memory_mapping,
        vm_addr as *const _ as * const u8 as u64,
        1,
        loader_id,
    ).unwrap()[0];

    let data = syscalls::translate_slice::<T>(
        memory_mapping,
        vec.as_slice() as *const _ as * const u8 as u64,
        vec.len() as u64,
        loader_id,
    ).unwrap();

    Vec::from(data)
}

pub fn read_memory (vm_memory :&Memory, memory_mapping: &MemoryMapping, loader_id: &Pubkey) -> Memory {
    let mut memory = &syscalls::translate_slice_mut::<Memory>(
        memory_mapping,
        vm_memory  as *const _ as * const u8 as u64,
        1,
        loader_id,
    ).unwrap()[0];

    let data = syscalls::translate_slice::<u8>(
        memory_mapping,
        memory.data_vec().as_slice() as *const _ as * const u8 as u64,
        memory.data_vec().len() as u64,
        loader_id,
    ).unwrap();

    Memory::from(data, memory.effective_len(), memory.limit())
}


pub fn read_stack (vm_stack :&Stack, memory_mapping: &MemoryMapping, loader_id: &Pubkey) -> Stack {
    let mut stack = &syscalls::translate_slice_mut::<Stack>(
        memory_mapping,
        vm_stack  as *const _ as * const u8 as u64,
        1,
        loader_id,
    ).unwrap()[0];

    let data = syscalls::translate_slice::<U256>(
        memory_mapping,
        stack.data_vec().as_slice() as *const _ as * const u8 as u64,
        stack.data_vec().len() as u64,
        loader_id,
    ).unwrap();

    Stack::from(data, stack.limit())
}


impl syscalls::EventListener for Tracer{
    fn save_bpf_units(&mut self, val: u64) {
        self.remaining = val;
        println!(" save remaining {}", val);
    }

    fn restore_bpf_units(&self) -> u64 {
        println!(" restore remaining {}", self.remaining);
        self.remaining
    }

    fn event(&mut self, vm_addr: u64, memory_mapping: &MemoryMapping, loader_id: &Pubkey ){

        let event = &mut syscalls::translate_slice_mut::<Event>(
                memory_mapping,
                vm_addr,
                1,
                loader_id,
            ).unwrap()[0];

        match event {
            Event::Call(trace) => {
                trace.transfer = &syscalls::translate_slice::<Option<Transfer>>(
                    memory_mapping,
                    trace.transfer as *const _ as * const u8 as u64,
                    1,
                    loader_id,
                ).unwrap()[0];

                let input = read_vec(trace.input, memory_mapping, loader_id);
                trace.input = &input;

                trace.context = &syscalls::translate_slice::<Context>(
                    memory_mapping,
                    trace.context as *const _ as * const u8 as u64,
                    1,
                    loader_id,
                ).unwrap()[0];
                println!("call: {:?} ", trace);
            },

            Event::Create(trace) =>  {
                let init_code = read_vec(trace.init_code, memory_mapping, loader_id);
                trace.init_code = &init_code;

                println!("create: {:?}", trace)
            },

            Event::Suicide(trace) =>  {
                println!("suicide {:?}", trace)
            },

            Event::Exit(trace) =>  {
                trace.reason = &syscalls::translate_slice::<ExitReason>(
                    memory_mapping,
                    trace.reason as *const _ as * const u8 as u64,
                    1,
                    loader_id,
                ).unwrap()[0];

                let return_value = read_vec(trace.return_value, memory_mapping, loader_id);
                trace.return_value = &return_value;
                println!("exit: {:?}", trace)
            },

            Event::TransactCall(trace) =>  {
                println!("transact_call");
                let data = read_vec(trace.data, memory_mapping, loader_id);
                trace.data = &data;
                println!(" transact_call {:?}", trace );
            },

            Event::TransactCreate(trace) =>  {
                println!("transact_create");
                let init_code = read_vec(trace.init_code, memory_mapping, loader_id);
                trace.init_code = &init_code;
                println!("transact_create {:?}", trace)
            },

            Event::TransactCreate2(trace) => {
                println!("transact_create2");
                let init_code = read_vec(trace.init_code, memory_mapping, loader_id);
                trace.init_code = &init_code;
                println!("transact_create2 {:?}", trace)
            },

            Event::Step(trace) =>  {
                trace.context = &syscalls::translate_slice::<Context>(
                    memory_mapping,
                    trace.context as *const _ as * const u8 as u64,
                    1,
                    loader_id,
                ).unwrap()[0];
                trace.position = &syscalls::translate_slice::<Result<usize, ExitReason>>(
                    memory_mapping,
                    trace.position as *const _ as * const u8 as u64,
                    1,
                    loader_id,
                ).unwrap()[0];

                let memory = read_memory(trace.memory, memory_mapping, loader_id);
                let stack = read_stack(trace.stack, memory_mapping, loader_id);

                trace.memory = &memory;
                trace.stack = &stack;
                println!("Step: {:?}", trace);
            },

            Event::StepResult(trace) =>  {
                let result = &syscalls::translate_slice::<Result<(), Capture<ExitReason, Trap>>>(
                    memory_mapping,
                    trace.result as *const _ as * const u8 as u64,
                    1,
                    loader_id,
                ).unwrap()[0];
                let return_value = read_vec(trace.return_value, memory_mapping, loader_id);
                let memory = read_memory(trace.memory, memory_mapping, loader_id);
                let stack = read_stack(trace.stack, memory_mapping, loader_id);

                trace.result = &result;
                trace.return_value = &return_value;
                trace.memory = &memory;
                trace.stack = &stack;
                println!("StepResult: {:?}", trace);
            },

            Event::SLoad(trace) =>  {
                println!("SLoad: {:?}", trace);
            },

            Event::SStore(trace) => {
                println!("SStore: {:?}", trace);
            }
        };
    }
}
