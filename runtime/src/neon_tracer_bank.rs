use {
    crate::{
        accounts::Accounts,
        accounts_db::AccountsDb,
        bank::{ Bank, BankFieldsToDeserialize, BankRc }, builtins::Builtins,
        dumper_db::{ DumperDb, DumperDbBank },
    },
    solana_sdk::{
        account::from_account,
        clock::Slot, genesis_config::ClusterType,
        genesis_config::GenesisConfig,
        hash::Hash,
        sysvar::{ self, rent::Rent, epoch_schedule::EpochSchedule },
        timing::years_as_slots,
    },

    std::{
        str::FromStr, sync::Arc,
    },
};

#[cfg(feature = "tracer")]
impl Bank {
    #[allow(clippy::float_cmp)]
    pub fn new_for_tracer(
        slot: Slot,
        cluster_type: ClusterType,
        dumper_db: Arc<DumperDb>,
        accounts_data_size_initial: u64,
        additional_builtins: Option<&Builtins>,
    ) -> Self {
        let recent_blockhashes = dumper_db.get_recent_blockhashes(slot, 12).unwrap();
        let epoch_schedule = dumper_db.load_account(&sysvar::epoch_schedule::id(), slot).unwrap();
        let epoch_schedule: EpochSchedule = from_account(&epoch_schedule).unwrap();
        let rent = dumper_db.load_account(&sysvar::rent::id(), slot).unwrap();
        let rent: Rent = from_account(&rent).unwrap();
        let block = dumper_db.get_block(slot).unwrap();

        let mut genesis_config = GenesisConfig::new(&[], &[]);
        genesis_config.cluster_type = cluster_type;
        genesis_config.epoch_schedule = epoch_schedule;
        genesis_config.rent = rent;

        let mut fields = BankFieldsToDeserialize::default();
        fields.epoch_schedule = epoch_schedule;
        fields.rent_collector.rent = rent;
        fields.rent_collector.epoch_schedule = fields.epoch_schedule;
        fields.rent_collector.epoch = fields.epoch_schedule.get_epoch(fields.slot);

        fields.hash = Hash::from_str(&block.blockhash).unwrap();
        fields.block_height = block.block_height.unwrap() as u64;

        fields.ticks_per_slot = genesis_config.ticks_per_slot;
        fields.ns_per_slot = genesis_config.poh_config.target_tick_duration.as_nanos()
            * genesis_config.ticks_per_slot as u128;
        fields.genesis_creation_time = genesis_config.creation_time;
        fields.max_tick_height = (fields.slot + 1) * fields.ticks_per_slot;
        fields.slots_per_year =
            years_as_slots(
                1.0,
                &genesis_config.poh_config.target_tick_duration,
                fields.ticks_per_slot,
            );

        for (current_slot, current_blockhash) in recent_blockhashes {
            let current_blockhash = Hash::from_str(current_blockhash.as_str()).unwrap();
            if current_slot == slot {
                fields.hash = current_blockhash;
            } else {
                fields.blockhash_queue.register_hash(&current_blockhash, 0);
            }
        }

        let mut accounts_db = AccountsDb::default_for_tests();
        accounts_db.dumper_db = DumperDbBank::new(dumper_db, slot);
        let bank_rc = BankRc::new(Accounts::new_empty(accounts_db), fields.slot);

        let bank = Bank::new_from_fields(
            bank_rc,
            &genesis_config,
            fields,
            None,
            additional_builtins,
            false,
            accounts_data_size_initial,
        );

        bank.fill_missing_sysvar_cache_entries();
        bank
    }

    pub fn dumper_db(&self) -> &DumperDbBank {
        &self.rc.accounts.accounts_db.dumper_db
    }

    pub fn set_enable_loading_from_dumper_db(&self, enable: bool) {
        self.rc.accounts.accounts_db.dumper_db.set_enable_loading_from_dumper_db(enable);
    }
}