pub mod create_account_v02;
pub mod call_from_raw_ethereum_tx;

use solana_sdk::{
    feature_set::{
        FeatureSet,
        tx_wide_compute_cap,
        requestable_heap_size,
        remove_native_loader,
        // demote_program_write_locks,
    },
    account::AccountSharedData,
    bpf_loader,
    native_loader,
    system_program,
    sysvar::instructions,
};


use solana_sdk::account::WritableAccount;


pub const evm_loader_str :&str = "eeLSJgWzzxrqKv1UxtRVVH8FX3qCQWUs9QuAjJpETGU";

pub fn feature_set() -> FeatureSet {
    let mut features = FeatureSet::all_enabled();
    features.deactivate(&tx_wide_compute_cap::id());
    features.deactivate(&requestable_heap_size ::id());
    features
}

pub fn bpf_loader_shared() -> AccountSharedData {
    let mut shared = AccountSharedData::new(1_000_000_000_000_000_000, 25, &native_loader::id());
    shared.set_executable(true);
    shared
}

pub fn evm_loader_shared() -> AccountSharedData {
    let mut shared = AccountSharedData::new(1_000_000_000_000_000_000, 36, &bpf_loader::id());
    shared.set_executable(true);
    shared
}

pub fn system_shared() -> AccountSharedData {
    let mut shared = AccountSharedData::new(1_000_000_000, 14, &native_loader::id());
    shared.set_executable(true);
    shared
}

pub fn sysvar_shared() -> AccountSharedData {
    let mut shared = AccountSharedData::new(1_000_000_000, 0, &instructions::id());
    shared.set_executable(true);
    shared
}



