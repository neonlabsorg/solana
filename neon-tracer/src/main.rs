use {
    solana_runtime::bank::Bank,
};
use solana_runtime::bank::BankRc;
use solana_runtime::accounts::Accounts;

pub fn main() {
    let slot = 0;
    let accounts = Accounts::new_for_tracer();
    let bank_rc = BankRc::new(accounts, slot);
    let bank = Bank::new_from_fields(
        bank_rc,)
    return;
}