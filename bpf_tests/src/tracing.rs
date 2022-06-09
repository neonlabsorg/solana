use evm::Event;
use solana_bpf_loader_program::syscalls as syscalls;


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

    fn event(&mut self, event: &[u8]){
        let event: Event = bincode::deserialize_from(event).unwrap();
        solana_program::msg!("EVENT EVENT EVENT EVENT  {:?} ", event);
        self.events.push(event)
    }
}
