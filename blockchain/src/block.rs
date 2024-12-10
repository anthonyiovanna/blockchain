use crate::crypto::Hash;
use crate::transaction::Transaction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub version: u32,
    pub timestamp: u64,
    pub prev_hash: Hash,
    pub merkle_root: Hash,
    pub difficulty: u32,
    pub nonce: u64,
}

impl Default for BlockHeader {
    fn default() -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        BlockHeader {
            version: 1,
            timestamp,
            prev_hash: Hash::new(&[0u8; 32]),
            merkle_root: Hash::new(&[0u8; 32]),
            difficulty: 1,
            nonce: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub hash: Hash,
}

impl Default for Block {
    fn default() -> Self {
        let header = BlockHeader::default();
        let transactions = Vec::new();
        let mut block = Block {
            header,
            transactions,
            hash: Hash::new(&[0u8; 32]),
        };
        block.hash = block.calculate_hash();
        block
    }
}

impl Block {
    pub fn new(
        version: u32,
        prev_hash: Hash,
        transactions: Vec<Transaction>,
        difficulty: u32,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let merkle_root = Self::calculate_merkle_root(&transactions);
        let mut block = Block {
            header: BlockHeader {
                version,
                timestamp,
                prev_hash,
                merkle_root,
                difficulty,
                nonce: 0,
            },
            transactions,
            hash: Hash::new(&[0u8; 32]), // Temporary hash
        };

        block.hash = block.calculate_hash();
        block
    }

    pub fn genesis() -> Self {
        let timestamp = 1640995200; // 2022-01-01 00:00:00 UTC
        let mut block = Block {
            header: BlockHeader {
                version: 1,
                timestamp,
                prev_hash: Hash::new(&[0u8; 32]),
                merkle_root: Hash::new(&[0u8; 32]),
                difficulty: 1,
                nonce: 0,
            },
            transactions: vec![],
            hash: Hash::new(&[0u8; 32]),
        };

        block.hash = block.calculate_hash();
        block
    }

    pub fn calculate_hash(&self) -> Hash {
        let mut data = Vec::new();
        data.extend_from_slice(&self.header.version.to_le_bytes());
        data.extend_from_slice(&self.header.timestamp.to_le_bytes());
        data.extend_from_slice(self.header.prev_hash.to_bytes());
        data.extend_from_slice(self.header.merkle_root.to_bytes());
        data.extend_from_slice(&self.header.difficulty.to_le_bytes());
        data.extend_from_slice(&self.header.nonce.to_le_bytes());
        
        // Also include transaction hashes in block hash calculation
        for tx in &self.transactions {
            data.extend_from_slice(tx.hash.to_bytes());
        }
        
        Hash::new(&data)
    }

    fn calculate_merkle_root(transactions: &[Transaction]) -> Hash {
        if transactions.is_empty() {
            return Hash::new(&[0u8; 32]);
        }

        let mut hashes: Vec<Hash> = transactions
            .iter()
            .map(|tx| tx.hash.clone())
            .collect();

        while hashes.len() > 1 {
            if hashes.len() % 2 != 0 {
                hashes.push(hashes.last().unwrap().clone());
            }

            let mut next_level = Vec::with_capacity(hashes.len() / 2);
            for chunk in hashes.chunks(2) {
                let mut data = Vec::new();
                data.extend_from_slice(chunk[0].to_bytes());
                data.extend_from_slice(chunk[1].to_bytes());
                next_level.push(Hash::new(&data));
            }
            hashes = next_level;
        }

        hashes[0].clone()
    }

    pub fn mine(&mut self) -> bool {
        let target = (1u128 << (128 - self.header.difficulty as u128)) - 1;
        
        while self.header.nonce < u64::MAX {
            let hash = self.calculate_hash();
            let hash_bytes = hash.to_bytes();
            let mut value = 0u128;
            
            // Convert first 16 bytes of hash to u128
            for i in 0..16 {
                value = (value << 8) | hash_bytes[i] as u128;
            }
            
            if value <= target {
                self.hash = hash;
                return true;
            }
            
            self.header.nonce += 1;
        }
        
        false
    }

    pub fn verify(&self) -> bool {
        // First verify the hash matches the block contents
        let calculated_hash = self.calculate_hash();
        if calculated_hash != self.hash {
            return false;
        }

        // Then verify the proof of work
        let target = (1u128 << (128 - self.header.difficulty as u128)) - 1;
        let hash_bytes = self.hash.to_bytes();
        let mut value = 0u128;
        
        for i in 0..16 {
            value = (value << 8) | hash_bytes[i] as u128;
        }
        
        // Finally verify the merkle root matches the transactions
        let merkle_root = Self::calculate_merkle_root(&self.transactions);
        if merkle_root != self.header.merkle_root {
            return false;
        }
        
        value <= target
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::{Transaction, TransactionInput, TransactionOutput};

    fn create_test_transaction() -> Transaction {
        Transaction::new(
            vec![TransactionInput {
                tx_hash: Hash::new(b"previous_tx"),
                output_index: 0,
                signature: None,
            }],
            vec![TransactionOutput {
                amount: 50,
                recipient: vec![1, 2, 3, 4],
            }],
        )
    }

    #[test]
    fn test_block_creation() {
        let prev_hash = Hash::new(b"previous block");
        let tx = create_test_transaction();
        let block = Block::new(1, prev_hash.clone(), vec![tx], 1);
        
        assert_eq!(block.header.version, 1);
        assert_eq!(block.header.prev_hash, prev_hash);
        assert_eq!(block.header.difficulty, 1);
        assert_eq!(block.transactions.len(), 1);
    }

    #[test]
    fn test_block_mining() {
        let mut block = Block::genesis();
        assert!(block.mine());
        assert!(block.verify());
    }

    #[test]
    fn test_merkle_root() {
        let tx1 = create_test_transaction();
        let tx2 = create_test_transaction();
        
        let block = Block::new(1, Hash::new(b"prev"), vec![tx1, tx2], 1);
        assert_ne!(block.header.merkle_root, Hash::new(&[0u8; 32]));
    }

    #[test]
    fn test_block_verification() {
        let mut block = Block::genesis();
        assert!(block.mine());
        assert!(block.verify());
        
        // Tamper with the block
        block.header.nonce += 1;
        // Don't update the hash - this should cause verification to fail
        assert!(!block.verify());
    }
}
