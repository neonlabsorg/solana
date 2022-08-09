use {
    postgres::{Client, NoTls, Statement},
};
use solana_sdk::account::AccountSharedData;
use solana_sdk::clock::Slot;
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Default)]
pub struct DumperDb {
}

impl DumperDb {
    pub fn load_account(&self, pubkey: &Pubkey, max_slot: Slot) -> Option<(AccountSharedData, Slot)> {
        todo!()
    }
}