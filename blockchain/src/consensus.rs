use crate::block::{Block, BlockHeader};
use crate::mempool::Mempool;
use crate::transaction::Transaction;
use crate::crypto::Hash;
use futures::future::join_all;
use std::error::Error;
use std::fmt;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum ConsensusError {
    ValidationError(String),
    BlockCreationError(String),
    TransactionError(String),
    NetworkError(String),
}

impl fmt::Display for ConsensusError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConsensusError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            ConsensusError::BlockCreationError(msg) => write!(f, "Block creation error: {}", msg),
            ConsensusError::TransactionError(msg) => write!(f, "Transaction error: {}", msg),
            ConsensusError::NetworkError(msg) => write!(f, "Network error: {}", msg),
        }
    }
}

impl Error for ConsensusError {}

#[async_trait::async_trait]
pub trait ConsensusEngine: Send + Sync {
    async fn validate_block(&self, block: &Block) -> Result<bool, ConsensusError>;
    async fn create_block(&self, mempool: &Mempool) -> Result<Block, ConsensusError>;
    async fn process_new_block(&self, block: Block) -> Result<(), ConsensusError>;
    fn get_difficulty(&self) -> u64;
}

pub struct ProofOfWork {
    difficulty: u64,
    max_block_size: usize,
}

impl ProofOfWork {
    pub fn new(difficulty: u64) -> Self {
        ProofOfWork {
            difficulty,
            max_block_size: 1000, // Maximum transactions per block
        }
    }

    async fn verify_transactions_parallel(&self, transactions: &[Transaction]) -> Result<bool, ConsensusError> {
        let (tx, mut rx) = mpsc::channel(100);
        
        let verification_tasks: Vec<_> = transactions
            .chunks(10) // Process in batches of 10
            .map(|batch| {
                let tx = tx.clone();
                let batch = batch.to_vec();
                tokio::spawn(async move {
                    for transaction in batch {
                        if let Err(e) = transaction.verify().await {
                            tx.send(Err(ConsensusError::TransactionError(e.to_string()))).await
                                .expect("Channel send failed");
                            return;
                        }
                    }
                    tx.send(Ok(())).await.expect("Channel send failed");
                })
            })
            .collect();

        // Wait for all verification tasks to complete
        join_all(verification_tasks).await;
        drop(tx);

        while let Some(result) = rx.recv().await {
            if let Err(e) = result {
                return Err(e);
            }
        }

        Ok(true)
    }

    fn check_difficulty(&self, hash: &[u8], difficulty: u64) -> bool {
        let mut count: u64 = 0;
        for byte in hash {
            let zeros = u64::from(byte.leading_zeros());
            count += zeros;
            if zeros < 8 {
                break;
            }
        }
        count >= difficulty
    }
}

#[async_trait::async_trait]
impl ConsensusEngine for ProofOfWork {
    async fn validate_block(&self, block: &Block) -> Result<bool, ConsensusError> {
        // Verify block hash meets difficulty requirement
        let hash = block.hash.to_bytes();
        if !self.check_difficulty(hash, self.difficulty) {
            return Err(ConsensusError::ValidationError("Block hash doesn't meet difficulty".into()));
        }

        // Parallel transaction verification
        self.verify_transactions_parallel(&block.transactions).await?;

        Ok(true)
    }

    async fn create_block(&self, mempool: &Mempool) -> Result<Block, ConsensusError> {
        // Get pending transactions from mempool
        let transactions = mempool.get_pending_transactions(self.max_block_size).await
            .map_err(|e| ConsensusError::BlockCreationError(e.to_string()))?;

        // Verify transactions in parallel
        self.verify_transactions_parallel(&transactions).await?;

        // Create new block
        let block = Block::new(
            1, // version
            Hash::new(&[0u8; 32]), // prev_hash (should be fetched from chain state)
            transactions,
            self.difficulty as u32,
        );

        Ok(block)
    }

    async fn process_new_block(&self, block: Block) -> Result<(), ConsensusError> {
        // Validate the block
        self.validate_block(&block).await?;

        // Additional processing like updating chain state would go here
        // For now we just return success
        Ok(())
    }

    fn get_difficulty(&self) -> u64 {
        self.difficulty
    }
}

pub struct ProofOfStake {
    min_stake: u64,
    max_block_size: usize,
}

impl ProofOfStake {
    pub fn new(min_stake: u64) -> Self {
        ProofOfStake {
            min_stake,
            max_block_size: 1000,
        }
    }

    async fn validate_pos(&self, _block: &Block) -> Result<bool, ConsensusError> {
        // Verify stake (placeholder implementation)
        // In a real implementation, this would verify stake ownership and delegation
        Ok(true)
    }

    fn validate_stake(&self, stake_amount: u64) -> bool {
        stake_amount >= self.min_stake
    }

    async fn verify_transactions_parallel(&self, transactions: &[Transaction]) -> Result<bool, ConsensusError> {
        let (tx, mut rx) = mpsc::channel(100);
        
        let verification_tasks: Vec<_> = transactions
            .chunks(10)
            .map(|batch| {
                let tx = tx.clone();
                let batch = batch.to_vec();
                tokio::spawn(async move {
                    for transaction in batch {
                        if let Err(e) = transaction.verify().await {
                            tx.send(Err(ConsensusError::TransactionError(e.to_string()))).await
                                .expect("Channel send failed");
                            return;
                        }
                    }
                    tx.send(Ok(())).await.expect("Channel send failed");
                })
            })
            .collect();

        join_all(verification_tasks).await;
        drop(tx);

        while let Some(result) = rx.recv().await {
            if let Err(e) = result {
                return Err(e);
            }
        }

        Ok(true)
    }
}

#[async_trait::async_trait]
impl ConsensusEngine for ProofOfStake {
    async fn validate_block(&self, block: &Block) -> Result<bool, ConsensusError> {
        // Verify PoS requirements
        self.validate_pos(block).await?;

        // Parallel transaction verification
        self.verify_transactions_parallel(&block.transactions).await?;

        Ok(true)
    }

    async fn create_block(&self, mempool: &Mempool) -> Result<Block, ConsensusError> {
        // Get pending transactions from mempool
        let transactions = mempool.get_pending_transactions(self.max_block_size).await
            .map_err(|e| ConsensusError::BlockCreationError(e.to_string()))?;

        // Verify transactions in parallel
        self.verify_transactions_parallel(&transactions).await?;

        // Create new block
        let block = Block::new(
            1, // version
            Hash::new(&[0u8; 32]), // prev_hash (should be fetched from chain state)
            transactions,
            1, // difficulty (less relevant for PoS)
        );

        Ok(block)
    }

    async fn process_new_block(&self, block: Block) -> Result<(), ConsensusError> {
        // Validate the block
        self.validate_block(&block).await?;

        // Additional processing like updating chain state would go here
        // For now we just return success
        Ok(())
    }

    fn get_difficulty(&self) -> u64 {
        // In PoS, difficulty is determined by stake amount
        self.min_stake
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pow_validation() {
        let pow = ProofOfWork::new(1); // Low difficulty for testing
        let block = Block::new(
            1,
            Hash::new(&[0u8; 32]),
            vec![],
            1,
        );

        // Test validation
        let result = pow.validate_block(&block).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_pos_validation() {
        let pos = ProofOfStake::new(1000);
        
        // Test stake validation
        assert!(pos.validate_stake(1500));
        assert!(!pos.validate_stake(500));

        let block = Block::new(
            1,
            Hash::new(&[0u8; 32]),
            vec![],
            1,
        );

        // Test block validation
        let result = pos.validate_block(&block).await.unwrap();
        assert!(result);
    }
}
