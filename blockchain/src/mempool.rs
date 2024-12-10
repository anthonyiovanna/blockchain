use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::transaction::Transaction;
use crate::crypto::Hash;

const DEFAULT_BATCH_SIZE: usize = 1000;

pub struct Mempool {
    transactions: Arc<RwLock<HashMap<Hash, Transaction>>>,
    seen_txs: Arc<RwLock<HashSet<Hash>>>,
    pending_queue: Arc<RwLock<VecDeque<(Transaction, Vec<Vec<u8>>)>>>,
    max_size: usize,
    batch_size: usize,
}

impl Mempool {
    pub fn new(max_size: usize) -> Self {
        Mempool {
            transactions: Arc::new(RwLock::new(HashMap::new())),
            seen_txs: Arc::new(RwLock::new(HashSet::new())),
            pending_queue: Arc::new(RwLock::new(VecDeque::new())),
            max_size,
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }

    pub fn with_batch_size(max_size: usize, batch_size: usize) -> Self {
        Mempool {
            transactions: Arc::new(RwLock::new(HashMap::new())),
            seen_txs: Arc::new(RwLock::new(HashSet::new())),
            pending_queue: Arc::new(RwLock::new(VecDeque::new())),
            max_size,
            batch_size,
        }
    }

    pub async fn add_transaction(&self, tx: Transaction, public_keys: Vec<Vec<u8>>) -> Result<bool, &'static str> {
        let tx_hash = tx.hash.clone();
        
        // Check if transaction was already seen
        {
            let seen = self.seen_txs.read().await;
            if seen.contains(&tx_hash) {
                return Ok(false);
            }
        }

        // Check mempool capacity
        {
            let txs = self.transactions.read().await;
            if txs.len() >= self.max_size {
                return Err("Mempool is full");
            }
        }

        // Add to pending queue
        {
            let mut queue = self.pending_queue.write().await;
            queue.push_back((tx, public_keys));
        }

        // Process pending queue if it reaches batch size
        self.process_pending_queue().await?;

        Ok(true)
    }

    async fn process_pending_queue(&self) -> Result<(), &'static str> {
        let mut batch = Vec::new();
        
        // Collect batch of transactions
        {
            let mut queue = self.pending_queue.write().await;
            while batch.len() < self.batch_size && !queue.is_empty() {
                if let Some(tx_data) = queue.pop_front() {
                    batch.push(tx_data);
                }
            }
        }

        if batch.is_empty() {
            return Ok(());
        }

        // Verify batch of transactions in parallel
        let verification_results = Transaction::verify_batch(&batch).await;

        // Process verification results
        let mut txs = self.transactions.write().await;
        let mut seen = self.seen_txs.write().await;

        for ((tx, _), result) in batch.into_iter().zip(verification_results) {
            match result {
                Ok(true) => {
                    let tx_hash = tx.hash.clone();
                    txs.insert(tx_hash.clone(), tx);
                    seen.insert(tx_hash);
                }
                Ok(false) => {
                    // Transaction verification failed, don't add to mempool
                    continue;
                }
                Err(_) => {
                    // Handle verification error
                    continue;
                }
            }
        }

        Ok(())
    }

    pub async fn get_pending_transactions(&self, limit: usize) -> Result<Vec<Transaction>, &'static str> {
        let txs = self.transactions.read().await;
        Ok(txs.values().take(limit).cloned().collect())
    }

    pub async fn process_all_pending(&self) -> Result<(), &'static str> {
        while !self.pending_queue.read().await.is_empty() {
            self.process_pending_queue().await?;
        }
        Ok(())
    }

    pub async fn remove_transaction(&self, hash: &Hash) -> Option<Transaction> {
        self.transactions.write().await.remove(hash)
    }

    pub async fn get_transaction(&self, hash: &Hash) -> Option<Transaction> {
        self.transactions.read().await.get(hash).cloned()
    }

    pub async fn get_all_transactions(&self) -> Vec<Transaction> {
        self.transactions.read().await.values().cloned().collect()
    }

    pub async fn clear_transactions(&self, hashes: &[Hash]) {
        let mut txs = self.transactions.write().await;
        for hash in hashes {
            txs.remove(hash);
        }
    }

    pub async fn contains(&self, hash: &Hash) -> bool {
        self.transactions.read().await.contains_key(hash)
    }

    pub async fn size(&self) -> usize {
        self.transactions.read().await.len()
    }

    pub async fn pending_size(&self) -> usize {
        self.pending_queue.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::{TransactionInput, TransactionOutput};
    use crate::crypto::KeyPair;

    #[tokio::test]
    async fn test_mempool_parallel_processing() {
        let mempool = Mempool::with_batch_size(100, 2);
        let keypair = KeyPair::generate();
        
        // Create test transactions
        let mut tx1 = Transaction::new(
            vec![TransactionInput {
                tx_hash: Hash::new(b"previous_tx_1"),
                output_index: 0,
                signature: None,
            }],
            vec![TransactionOutput {
                amount: 100,
                recipient: vec![1, 2, 3, 4],
            }],
        );
        tx1.sign(&keypair, 0).unwrap();

        let mut tx2 = Transaction::new(
            vec![TransactionInput {
                tx_hash: Hash::new(b"previous_tx_2"),
                output_index: 0,
                signature: None,
            }],
            vec![TransactionOutput {
                amount: 200,
                recipient: vec![1, 2, 3, 4],
            }],
        );
        tx2.sign(&keypair, 0).unwrap();

        // Add transactions
        let public_keys = vec![keypair.public_key().as_bytes().to_vec()];
        assert!(mempool.add_transaction(tx1.clone(), public_keys.clone()).await.unwrap());
        assert!(mempool.add_transaction(tx2.clone(), public_keys.clone()).await.unwrap());
        
        // Process all pending transactions
        mempool.process_all_pending().await.unwrap();

        // Verify transactions were added
        assert_eq!(mempool.size().await, 2);
        assert!(mempool.contains(&tx1.hash).await);
        assert!(mempool.contains(&tx2.hash).await);
    }

    #[tokio::test]
    async fn test_mempool_duplicate_prevention() {
        let mempool = Mempool::new(100);
        let keypair = KeyPair::generate();
        
        let mut tx = Transaction::new(
            vec![TransactionInput {
                tx_hash: Hash::new(b"previous_tx"),
                output_index: 0,
                signature: None,
            }],
            vec![TransactionOutput {
                amount: 100,
                recipient: vec![1, 2, 3, 4],
            }],
        );
        tx.sign(&keypair, 0).unwrap();

        let public_keys = vec![keypair.public_key().as_bytes().to_vec()];
        
        // Add transaction first time
        assert!(mempool.add_transaction(tx.clone(), public_keys.clone()).await.unwrap());
        mempool.process_all_pending().await.unwrap();
        
        // Try to add same transaction again
        assert!(!mempool.add_transaction(tx.clone(), public_keys.clone()).await.unwrap());
        
        assert_eq!(mempool.size().await, 1);
    }

    #[tokio::test]
    async fn test_mempool_max_size() {
        let max_size = 2;
        let mempool = Mempool::new(max_size);
        let keypair = KeyPair::generate();
        let public_keys = vec![keypair.public_key().as_bytes().to_vec()];

        // Add transactions up to max size
        for i in 0..max_size {
            let mut tx = Transaction::new(
                vec![TransactionInput {
                    tx_hash: Hash::new(format!("tx_{}", i).as_bytes()),
                    output_index: 0,
                    signature: None,
                }],
                vec![TransactionOutput {
                    amount: 100,
                    recipient: vec![1, 2, 3, 4],
                }],
            );
            tx.sign(&keypair, 0).unwrap();
            assert!(mempool.add_transaction(tx, public_keys.clone()).await.unwrap());
        }

        mempool.process_all_pending().await.unwrap();

        // Try to add one more transaction
        let mut overflow_tx = Transaction::new(
            vec![TransactionInput {
                tx_hash: Hash::new(b"overflow_tx"),
                output_index: 0,
                signature: None,
            }],
            vec![TransactionOutput {
                amount: 100,
                recipient: vec![1, 2, 3, 4],
            }],
        );
        overflow_tx.sign(&keypair, 0).unwrap();
        assert!(mempool.add_transaction(overflow_tx, public_keys.clone()).await.is_err());
    }

    #[tokio::test]
    async fn test_get_pending_transactions() {
        let mempool = Mempool::new(100);
        let keypair = KeyPair::generate();
        let public_keys = vec![keypair.public_key().as_bytes().to_vec()];

        // Add some transactions
        for i in 0..5 {
            let mut tx = Transaction::new(
                vec![TransactionInput {
                    tx_hash: Hash::new(format!("tx_{}", i).as_bytes()),
                    output_index: 0,
                    signature: None,
                }],
                vec![TransactionOutput {
                    amount: 100,
                    recipient: vec![1, 2, 3, 4],
                }],
            );
            tx.sign(&keypair, 0).unwrap();
            mempool.add_transaction(tx, public_keys.clone()).await.unwrap();
        }

        mempool.process_all_pending().await.unwrap();

        // Test getting pending transactions with different limits
        let txs = mempool.get_pending_transactions(3).await.unwrap();
        assert_eq!(txs.len(), 3);

        let all_txs = mempool.get_pending_transactions(10).await.unwrap();
        assert_eq!(all_txs.len(), 5);
    }
}
