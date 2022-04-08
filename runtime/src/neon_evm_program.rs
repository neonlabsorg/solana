solana_sdk::declare_id!("53DfF883gyixYNXnM7s5xhdeyV8mVk9T4i2hGV9vG9io");

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_id() {
        assert!(check_id(&id()));
        id().log();
    }
}
