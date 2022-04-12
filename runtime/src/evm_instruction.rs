use evm::{H160, U256};
use evm_loader::utils::keccak256_digest;
use solana_sdk::secp256k1_recover::{secp256k1_recover, Secp256k1RecoverError};

#[derive(Debug)]
pub struct SignedTransaction<'a> {
    pub unsigned: UnsignedTransaction,
    pub signature: &'a [u8],
}

// TODO: import this from `evm_loader::transaction` as soon as it becomes public
#[derive(Debug)]
pub struct UnsignedTransaction {
    pub nonce: u64,
    pub gas_price: U256,
    pub gas_limit: U256,
    pub to: Option<H160>,
    pub value: U256,
    pub call_data: Vec<u8>,
    pub chain_id: U256,
}

impl rlp::Encodable for SignedTransaction<'_> {
    fn rlp_append(&self, s: &mut rlp::RlpStream) {
        s.begin_list(9);
        s.append(&self.unsigned.nonce);
        s.append(&self.unsigned.gas_price);
        s.append(&self.unsigned.gas_limit);
        match self.unsigned.to.as_ref() {
            None => s.append(&""),
            Some(addr) => s.append(addr),
        };
        s.append(&self.unsigned.value);
        s.append(&self.unsigned.call_data);
        s.append(
            &(U256::from(self.signature[64])
                + U256::from(35)
                + U256::from(2) * self.unsigned.chain_id),
        );
        s.append(&self.signature[..32].as_ref());
        s.append(&self.signature[32..64].as_ref());
        /*
        s.append(&self.chain_id);
        s.append_empty_data();
        s.append_empty_data();
        */
    }
}

impl rlp::Decodable for UnsignedTransaction {
    fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
        if rlp.item_count()? != 9 {
            return Err(rlp::DecoderError::RlpIncorrectListLen);
        }

        let tx = Self {
            nonce: rlp.val_at(0)?,
            gas_price: rlp.val_at(1)?,
            gas_limit: rlp.val_at(2)?,
            to: {
                let to = rlp.at(3)?;
                if to.is_empty() {
                    if to.is_data() {
                        None
                    } else {
                        return Err(rlp::DecoderError::RlpExpectedToBeData);
                    }
                } else {
                    Some(to.as_val()?)
                }
            },
            value: rlp.val_at(4)?,
            call_data: rlp.val_at(5)?,
            chain_id: rlp.val_at(6)?,
        };

        Ok(tx)
    }
}

// TODO: import this from `evm_loader::transaction` as soon as it becomes public
#[allow(unused)]
pub fn verify_tx_signature(
    signature: &[u8],
    unsigned_trx: &[u8],
) -> Result<H160, Secp256k1RecoverError> {
    let digest = keccak256_digest(unsigned_trx);

    let public_key = secp256k1_recover(&digest, signature[64], &signature[0..64])?;

    let address = keccak256_digest(&public_key.to_bytes());
    let address = H160::from_slice(&address[12..32]);

    Ok(address)
}
