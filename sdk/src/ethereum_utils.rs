use clarity::{types::BigEndianInt,};
use serde::{Deserialize, Deserializer};

struct EthereumTransaction(clarity::Transaction);

impl<'de> Deserialize<'de> for EthereumTransaction {
    fn deserialize<D>(deserializer: D) -> Result<EthereumTransaction, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(EthereumTransaction {

        })
    }
}

pub fn get_tx_sender(eth_tx: &[u8]) -> u64 {
    // The implementation. Can be called from BPF loader, not from program directly
    #[cfg(not(all(feature = "program", target_arch = "bpf")))]
    {
        eth_tx.len() as u64
    }
    // Make BPF loader system call, which, in its turn, will call the implementation above
    #[cfg(all(feature = "program", target_arch = "bpf"))]
    {
        extern "C" {
            fn sol_eth_decode_tx(
                eth_tx: *const u8,
                eth_tx_len: u64,
            ) -> u64;
        };
        let result = unsafe {
            sol_eth_decode_tx(
                eth_tx as *const _ as *const u8,
                eth_tx.len() as u64,
            )
        };
        result
    }
}
