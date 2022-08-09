use std::sync;
use {
    solana_runtime::{
        bank::Bank,
        dumper_db::DumperDb,
    },
    sync::Arc,
};

pub fn main() {
    let dumper_db = Arc::<DumperDb>::default();
    let bank = Bank::new_for_tracer(dumper_db, 0);
    return;
}