//solana_sdk::declare_id!("53DfF883gyixYNXnM7s5xhdeyV8mVk9T4i2hGV9vG9io");
solana_sdk::declare_id!("eeLSJgWzzxrqKv1UxtRVVH8FX3qCQWUs9QuAjJpETGU");

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_id() {
        id().log();
    }
}
