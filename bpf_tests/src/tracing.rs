use evm::tracing::{EventOnStack, Event};
use solana_bpf_loader_program::syscalls as syscalls;
use solana_rbpf::memory_region::MemoryMapping;
use solana_sdk::pubkey::Pubkey;
use std::{
    slice::from_raw_parts,
    mem::size_of,
};

pub struct Tracer{
    events: Vec<Event>,
}

impl Tracer  {
    pub fn new() -> Self {
        Tracer {
            events: vec![],
        }
    }
}

impl syscalls::EventListener for Tracer{

    // fn event(&mut self, event: &[u8]){
    //     let event: Event = bincode::deserialize_from(event).unwrap();
    //     solana_program::msg!("EVENT EVENT EVENT EVENT  {:?} ", event);
    //     self.events.push(event)
    // }

    fn event(&mut self, vm_addr: u64, memory_mapping: &MemoryMapping, loader_id: &Pubkey ){
        // let event: Event = bincode::deserialize_from(event).unwrap();
        solana_program::msg!("EVENT EVENT EVENT EVENT  {:?} ", vm_addr);
        // self.events.push(event)

        let value = syscalls::translate_slice::<EventOnStack>(
                memory_mapping,
                vm_addr,
                // size_of::<EventOnStack>() as u64,
                1,
                loader_id,
            ).unwrap();

        match &value[0] {
            EventOnStack::Call(trace) =>  println!(" call " ),
            EventOnStack::Create(trace) =>  println!(" create " ),
            EventOnStack::Suicide(trace) =>  println!(" suicide " ),
            EventOnStack::Exit(trace) =>  println!(" exit " ),
            EventOnStack::TransactCall(trace) =>  println!(" transact_call " ),
            EventOnStack::TransactCreate(trace) =>  println!(" transact_create " ),
            EventOnStack::TransactCreate2(trace) =>  println!(" transact_crate2 " ),
            EventOnStack::Step(trace) =>  println!(" step {:?}", trace.context ),
            EventOnStack::StepResult(trace) =>  println!(" step_result " ),
            EventOnStack::SLoad(trace) =>  println!(" ssload " ),
            EventOnStack::SStore(trace) =>  println!(" sstore " ),
        }
        // println!("{:?}", value[0].);
        // let ptr = value[0] as * const EventOnStack;

        // unsafe {
        //     let val: &[EventOnStack] = from_raw_parts(ptr, 1);
        //     // let val = &*ptr;
        //     let a = val[0].clone();
        //
        //     match &val[0] {
        //         EventOnStack::Call(trace) => {
        //             // println!(" trace {:?}", trace.code_address )
        //             println!(" trace " )
        //         },
        //         _ => println!(" other " )
        //     }
        // }

    }
}
