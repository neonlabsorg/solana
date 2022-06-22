pub mod create_account_v02;
pub mod call_from_raw_ethereum_tx;
pub mod keccak_secp256k1;

use std::sync::Arc;

use solana_sdk::{
    feature_set::{
        FeatureSet,
        tx_wide_compute_cap,
        requestable_heap_size,
        remove_native_loader,
        prevent_calling_precompiles_as_programs,
        // demote_program_write_locks,
    },
    account::AccountSharedData,
    bpf_loader,
    native_loader,
    system_program,
    sysvar::instructions,
    bpf_loader_upgradeable,
};
use solana_program:: {
    pubkey::Pubkey,
    keccak::hash,
};

use evm::{H160, U256};

use evm_loader::{
    account::{
        ether_account,
        ether_contract,
        Packable,
        AccountData,
    },
    config::{
        collateral_pool_base,
        CHAIN_ID,
        AUTHORIZED_OPERATOR_LIST,
    },

};

use libsecp256k1::{SecretKey, Signature};

use rlp::RlpStream;

use solana_sdk::account::WritableAccount;


// #[derive(Debug)]
struct UnsignedTransaction {
    nonce: u64,
    gas_price: U256,
    gas_limit: U256,
    to: Option<H160>,
    value: U256,
    data: Vec<u8>,
    chain_id: U256,
}


pub const evm_loader_str :&str = "eeLSJgWzzxrqKv1UxtRVVH8FX3qCQWUs9QuAjJpETGU";
pub const evm_loader_orig_str :&str = "31QHZZ2azAyK7NsGUdw3kxhG9AJaiQ1ExUvcJiMEQ8k9";

pub fn feature_set() -> Arc<FeatureSet> {
    let mut features = FeatureSet::all_enabled();
    features.deactivate(&tx_wide_compute_cap::id());
    features.deactivate(&requestable_heap_size ::id());
    // features.deactivate(&prevent_calling_precompiles_as_programs ::id());
    Arc::new(features)
}

pub fn bpf_loader_shared() -> AccountSharedData {
    let mut shared = AccountSharedData::new(1_000_000_000_000_000_000, 25, &native_loader::id());
    shared.set_executable(true);
    shared
}

pub fn system_shared() -> AccountSharedData {
    let mut shared = AccountSharedData::new(1_000_000_000, 14, &native_loader::id());
    shared.set_executable(true);
    shared
}

pub fn sysvar_shared() -> AccountSharedData {
    let mut shared = AccountSharedData::new(1_000_000_000, 100, &instructions::id());
    shared.set_executable(true);
    shared
}


impl rlp::Encodable for UnsignedTransaction {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(9);
        s.append(&self.nonce);
        s.append(&self.gas_price);
        s.append(&self.gas_limit);
        match self.to.as_ref() {
            None => s.append(&""),
            Some(addr) => s.append(addr),
        };
        s.append(&self.value);
        s.append(&self.data);
        s.append(&self.chain_id);
        s.append_empty_data();
        s.append_empty_data();
    }
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    hash(data).to_bytes()
}

pub fn make_ethereum_transaction(
    trx_count: u64,
    to: &H160,
) -> (Vec<u8>, Vec<u8>) {

    let pk_hex: &[u8] = "0510266f7d37f0957564e4ce1a1dcc8bb3408383634774a2f4a94a35f4bc53e0".as_bytes();
    let mut bin : [u8; 32] = [0; 32];
    bin.copy_from_slice( hex::decode(&pk_hex).unwrap().as_slice());

    let pk = SecretKey::parse(&bin).unwrap();

    // let call = "8d0357794e";  // callHelloWorld()
    let call = "3917b3df";  // callHelloWorld()
    let data = hex::decode(call).unwrap().as_slice().to_vec();

    let rlp_data = {
        let tx = UnsignedTransaction {
            to: Some(*to),
            nonce: trx_count,
            gas_limit: 9_999_999_999_u64.into(),
            gas_price: 10_u64.pow(0).into(),
            value: U256::zero(),
            // value: U256::from(10_u64),
            data: data.to_vec(),
            // data: vec![],
            chain_id: CHAIN_ID.into(),
        };

        rlp::encode(&tx).to_vec()
    };

    let (r_s, v) = {
        use libsecp256k1::{Message, sign};
        let msg = libsecp256k1::Message::parse(&keccak256(rlp_data.as_slice()));
        libsecp256k1::sign(&msg, &pk)
    };

    let mut signature : Vec<u8> = Vec::new();
    signature.extend(r_s.serialize().iter().copied());
    signature.push(v.serialize());

    (signature, rlp_data)
}



