#[cfg(feature = "tracer")]
use {
    crate::{
        accounts::Accounts,
        accounts_db::AccountsDb,
        bank::{ Bank, BankFieldsToDeserialize, BankRc, TransactionSimulationResult },
        builtins::Builtins,
        neon_dumperdb::{ DumperDb, DumperDbError },
        neon_dumperdb_bank::DumperDbBank
    },
    solana_sdk::{
        account::{ AccountSharedData, from_account },
        clock::Slot, genesis_config::ClusterType,
        genesis_config::GenesisConfig,
        hash::{ Hash, ParseHashError },
        pubkey::Pubkey,
        sysvar::{ self },
        timing::years_as_slots,
        transaction::SanitizedTransaction,
    },
    std::{ collections::BTreeMap, str::FromStr, sync::Arc },
    thiserror::Error,
};

#[cfg(feature = "tracer")]
#[derive(Error, Debug)]
pub enum BankCreationError {
    #[error("Failed to load recent blockhashes: {err}")]
    FailedLoadBlockhashes{ err: DumperDbError },

    #[error("Failed to load epoch schedule: {err}")]
    FailedLoadEpochSchedule{ err: DumperDbError },

    #[error("Failed to parse epoch schedule account")]
    FailedParseEpochSchedule,

    #[error("Failed to load rent: {err}")]
    FailedLoadRent{ err: DumperDbError },

    #[error("Failed to parse rent account")]
    FailedParseRent,

    #[error("Failed to load block: {err}")]
    FailedLoadBlock{ err: DumperDbError },

    #[error("Failed to parse hash: {err}")]
    FailedParseHash{ err: ParseHashError },

    #[error("Block height not specified")]
    BlockHeightNotSpecified,
}

#[cfg(feature = "tracer")]
impl Bank {
    #[allow(clippy::float_cmp)]
    pub fn new_for_tracer(
        slot: Slot,
        cluster_type: ClusterType,
        dumper_db: Arc<DumperDb>,
        accounts_data_size_initial: u64,
        additional_builtins: Option<&Builtins>,
    ) -> Result<Self, BankCreationError> {
        let recent_blockhashes = dumper_db.get_recent_blockhashes(slot, 12)
            .map_err(|err| BankCreationError::FailedLoadBlockhashes { err })?;

        let epoch_schedule = dumper_db.load_account(&sysvar::epoch_schedule::id(), slot)
            .map_err(|err| BankCreationError::FailedLoadEpochSchedule { err })?;
        let epoch_schedule = from_account(&epoch_schedule)
            .map_or_else(
                || Err(BankCreationError::FailedParseEpochSchedule),
                |value| Ok(value)
            )?;

        let rent = dumper_db.load_account(&sysvar::rent::id(), slot)
            .map_err(|err| BankCreationError::FailedLoadRent { err })?;
        let rent = from_account(&rent)
            .map_or_else(
                || Err(BankCreationError::FailedParseRent),
                |value| Ok(value)
            )?;

        let block = dumper_db.get_block(slot)
            .map_err(|err| BankCreationError::FailedLoadBlock { err })?;

        let mut genesis_config = GenesisConfig::new(&[], &[]);
        genesis_config.cluster_type = cluster_type;
        genesis_config.epoch_schedule = epoch_schedule;
        genesis_config.rent = rent;

        let mut fields = BankFieldsToDeserialize::default();
        fields.epoch_schedule = epoch_schedule;
        fields.rent_collector.rent = rent;
        fields.rent_collector.epoch_schedule = fields.epoch_schedule;
        fields.rent_collector.epoch = fields.epoch_schedule.get_epoch(fields.slot);

        fields.hash = Hash::from_str(&block.blockhash)
            .map_err(|err| BankCreationError::FailedParseHash { err })?;

        if let Some(block_height) = block.block_height {
            fields.block_height = block_height as u64;
        } else {
            return Err(BankCreationError::BlockHeightNotSpecified);
        }

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
            let current_blockhash = Hash::from_str(current_blockhash.as_str())
                .map_err(|err| BankCreationError::FailedParseHash { err })?;

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
        Ok(bank)
    }

    pub fn replay_transaction(
        &self,
        trx: SanitizedTransaction,
        accounts: &BTreeMap<Pubkey, AccountSharedData>
    ) -> Option<TransactionSimulationResult> {
        if !self.rc.accounts.accounts_db.dumper_db.load_accounts_to_cache(accounts) {
            return None;
        }

        self.rc.accounts.accounts_db.dumper_db.set_enable_loading_from_dumper_db(false);
        let result = self.simulate_transaction(trx);
        self.rc.accounts.accounts_db.dumper_db.set_enable_loading_from_dumper_db(true);
        Some(result)
    }
}
