use {
    crate::{ ancestors::Ancestors, dumper_db::DumperDb },
    solana_sdk::{
        clock::Slot,
        pubkey::Pubkey,
        account::AccountSharedData,
    },
    log::*,
    std::{ collections::BTreeMap, sync::{ Arc, Mutex } },
};

#[derive(Debug, Default)]
pub struct EnableLoading{
    pub value: bool
}

impl EnableLoading {
    pub fn set_enable_loading(&mut self, enable: bool) {
        self.value = enable;
    }
}

#[derive(Debug, Default)]
pub struct DumperDbBank {
    pub dumper_db: Option<Arc<DumperDb>>,
    pub slot: Slot,
    pub account_cache: Mutex<BTreeMap<Pubkey, AccountSharedData>>,
    pub enable_loading_from_db: Mutex<EnableLoading>,
}

impl DumperDbBank {
    pub fn new(dumper_db: Arc<DumperDb>, slot: Slot) -> Self {
        DumperDbBank {
            dumper_db: Some(dumper_db),
            slot,
            account_cache: Mutex::new(BTreeMap::new()),
            enable_loading_from_db: Mutex::new(EnableLoading{ value: true }),
        }
    }

    pub fn set_enable_loading_from_dumper_db(&self, enable: bool) {
        if let Ok(mut enable_loading) = self.enable_loading_from_db.lock() {
            enable_loading.set_enable_loading(enable);
        }
    }

    pub fn take_cache_snapshot(&self) -> Option<BTreeMap<Pubkey, AccountSharedData>> {
        let account_cache = self.account_cache.lock();
        match account_cache {
            Err(err) => {
                let msg = format!("Failed to obtain account-cache lock: {}", err);
                error!("{}", msg);
                None
            }
            Ok(mut account_cache) => {
                Some(account_cache.clone())
            }
        }
    }

    pub fn clear_cache(&self) -> bool {
        let account_cache = self.account_cache.lock();
        match account_cache {
            Err(err) => {
                let msg = format!("Failed to obtain account-cache lock: {}", err);
                error!("{}", msg);
                false
            }
            Ok(mut account_cache) => {
                account_cache.clear();
                debug!("Account cache cleared");
                true
            }
        }
    }

    pub fn load_accounts_to_cache(&self, snapshot: &BTreeMap<Pubkey, AccountSharedData>) -> bool {
        let account_cache = self.account_cache.lock();
        match account_cache {
            Err(err) => {
                let msg = format!("Failed to obtain account-cache lock: {}", err);
                error!("{}", msg);
                false
            }
            Ok(mut account_cache) => {
                for (pubkey, account) in snapshot {
                    account_cache.insert(pubkey.clone(), account.clone());
                }
                debug!("Account cache: Loading finished");
                true
            }
        }
    }

    pub fn load_account(
        &self,
        ancestors: &Ancestors,
        pubkey: &Pubkey,
        max_root: Option<Slot>
    ) -> Option<(AccountSharedData, Slot)> {
        let account_cache = self.account_cache.lock();
        let enable_loading = self.enable_loading_from_db.lock();
        if enable_loading.is_err() {
            return None;
        }
        match account_cache {
            Err(err) => {
                let msg = format!("Failed to obtain account-cache lock: {}", err);
                error!("{}", msg);
                return None;
            }
            Ok(mut account_cache) => {
                if let Some(account) = account_cache.get(pubkey) {
                    debug!("Account {} found in cache", pubkey);
                    return Some((account.clone(), self.slot))
                }

                if enable_loading.unwrap().value {
                    if let Ok(account) = self.dumper_db.as_ref().unwrap().load_account(pubkey, self.slot) {
                        debug!("Account {} loaded from DB", pubkey);
                        account_cache.insert(*pubkey, account.clone());
                        return Some((account, self.slot))
                    }

                    let msg = format!("Unable to read account {} from dumper-db", pubkey.to_string());
                    error!("{}", msg);
                }

                None
            }
        }
    }
}
