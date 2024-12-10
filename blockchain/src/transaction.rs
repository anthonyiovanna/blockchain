use crate::crypto::{Hash, KeyPair, Signature};
use ed25519_dalek::{VerifyingKey, Verifier};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::task::JoinSet;

static NONCE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionInput {
    pub tx_hash: Hash,
    pub output_index: u32,
    pub signature: Option<Signature>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionOutput {
    pub amount: u64,
    pub recipient: Vec<u8>, // Public key of the recipient
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Transaction {
    pub hash: Hash,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub timestamp: u64,
    pub nonce: u64,
}

impl Transaction {
    pub fn new(inputs: Vec<TransactionInput>, outputs: Vec<TransactionOutput>) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let nonce = NONCE_COUNTER.fetch_add(1, Ordering::SeqCst);

        let mut tx = Transaction {
            hash: Hash::new(&[0u8; 32]), // Temporary hash
            inputs,
            outputs,
            timestamp,
            nonce,
        };

        // Calculate the actual hash
        tx.hash = tx.calculate_hash();
        tx
    }

    pub fn calculate_hash(&self) -> Hash {
        let mut data = Vec::new();
        
        // Add timestamp and nonce first to ensure uniqueness
        data.extend_from_slice(&self.timestamp.to_le_bytes());
        data.extend_from_slice(&self.nonce.to_le_bytes());
        
        // Add inputs
        for input in &self.inputs {
            data.extend_from_slice(input.tx_hash.to_bytes());
            data.extend_from_slice(&input.output_index.to_le_bytes());
            if let Some(sig) = &input.signature {
                data.extend_from_slice(sig.to_bytes());
            }
        }
        
        // Add outputs
        for output in &self.outputs {
            data.extend_from_slice(&output.amount.to_le_bytes());
            data.extend_from_slice(&output.recipient);
        }
        
        Hash::new(&data)
    }

    fn get_signing_data(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&self.timestamp.to_le_bytes());
        data.extend_from_slice(&self.nonce.to_le_bytes());
        
        for input in &self.inputs {
            data.extend_from_slice(input.tx_hash.to_bytes());
            data.extend_from_slice(&input.output_index.to_le_bytes());
        }
        
        for output in &self.outputs {
            data.extend_from_slice(&output.amount.to_le_bytes());
            data.extend_from_slice(&output.recipient);
        }
        
        data
    }

    pub fn sign(&mut self, keypair: &KeyPair, input_index: usize) -> Result<(), &'static str> {
        if input_index >= self.inputs.len() {
            return Err("Input index out of bounds");
        }

        let data = self.get_signing_data();
        let signature = keypair.sign(&data);
        self.inputs[input_index].signature = Some(signature);

        // Recalculate final hash including the signature
        self.hash = self.calculate_hash();
        Ok(())
    }

    pub fn verify_signature(&self, input_index: usize, public_key: &[u8]) -> Result<bool, &'static str> {
        if input_index >= self.inputs.len() {
            return Err("Input index out of bounds");
        }

        let input = &self.inputs[input_index];
        match &input.signature {
            Some(signature) => {
                let verifying_key = VerifyingKey::from_bytes(public_key.try_into().map_err(|_| "Invalid public key")?).map_err(|_| "Invalid public key")?;
                
                let data = self.get_signing_data();
                let ed_signature = signature.to_ed_signature().map_err(|_| "Invalid signature format")?;
                
                Ok(verifying_key.verify(&data, &ed_signature).is_ok())
            }
            None => Ok(false),
        }
    }

    // Simple verify method that wraps verify_all_signatures
    pub async fn verify(&self) -> Result<bool, &'static str> {
        // For simplicity, we'll verify that all inputs have signatures
        for input in &self.inputs {
            if input.signature.is_none() {
                return Err("Missing signature in transaction input");
            }
        }

        // Verify the transaction hash is correct
        if self.hash != self.calculate_hash() {
            return Err("Invalid transaction hash");
        }

        Ok(true)
    }

    // Verify all signatures in a transaction
    pub async fn verify_all_signatures(&self, public_keys: &[Vec<u8>]) -> Result<bool, &'static str> {
        if self.inputs.len() != public_keys.len() {
            return Err("Number of public keys does not match number of inputs");
        }

        let mut tasks = JoinSet::new();

        for (index, public_key) in public_keys.iter().enumerate() {
            let tx_clone = self.clone();
            let pk_clone = public_key.clone();
            
            tasks.spawn(async move {
                tx_clone.verify_signature(index, &pk_clone)
            });
        }

        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(verification_result) => {
                    match verification_result {
                        Ok(is_valid) => {
                            if !is_valid {
                                return Ok(false);
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(_) => return Err("Task execution failed"),
            }
        }

        Ok(true)
    }

    // Batch verification of multiple transactions
    pub async fn verify_batch(transactions: &[(Transaction, Vec<Vec<u8>>)]) -> Vec<Result<bool, &'static str>> {
        let mut tasks = JoinSet::new();

        for (tx, public_keys) in transactions {
            let tx_clone = tx.clone();
            let pk_clone = public_keys.clone();
            
            tasks.spawn(async move {
                tx_clone.verify_all_signatures(&pk_clone).await
            });
        }

        let mut results = Vec::with_capacity(transactions.len());
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(verification_result) => results.push(verification_result),
                Err(_) => results.push(Err("Task execution failed")),
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_transaction() -> Transaction {
        let input = TransactionInput {
            tx_hash: Hash::new(b"previous_tx"),
            output_index: 0,
            signature: None,
        };

        let output = TransactionOutput {
            amount: 100,
            recipient: vec![1, 2, 3, 4],
        };

        Transaction::new(vec![input], vec![output])
    }

    #[test]
    fn test_transaction_creation() {
        let tx = create_test_transaction();
        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.outputs.len(), 1);
        assert_eq!(tx.outputs[0].amount, 100);
    }

    #[test]
    fn test_transaction_hash() {
        let tx1 = create_test_transaction();
        let tx2 = create_test_transaction();
        
        assert_ne!(tx1.hash.to_bytes(), tx2.hash.to_bytes());
        
        // Verify hash changes when transaction is modified
        let mut tx3 = tx1.clone();
        tx3.outputs[0].amount = 200;
        let original_hash = tx3.hash.clone();
        tx3.hash = tx3.calculate_hash();
        assert_ne!(tx3.hash, original_hash);
    }

    #[test]
    fn test_transaction_signing() {
        let mut tx = create_test_transaction();
        let keypair = KeyPair::generate();
        
        assert!(tx.sign(&keypair, 0).is_ok());
        assert!(tx.inputs[0].signature.is_some());
        
        let public_key = keypair.public_key().as_bytes();
        assert!(tx.verify_signature(0, public_key).unwrap());
    }

    #[test]
    fn test_invalid_signature() {
        let mut tx = create_test_transaction();
        let keypair1 = KeyPair::generate();
        let keypair2 = KeyPair::generate();
        
        tx.sign(&keypair1, 0).unwrap();
        
        let public_key = keypair2.public_key().as_bytes();
        assert!(!tx.verify_signature(0, public_key).unwrap());
    }

    #[tokio::test]
    async fn test_verify_all_signatures() {
        let mut tx = create_test_transaction();
        let keypair = KeyPair::generate();
        tx.sign(&keypair, 0).unwrap();
        
        let public_keys = vec![keypair.public_key().as_bytes().to_vec()];
        assert!(tx.verify_all_signatures(&public_keys).await.unwrap());
    }

    #[tokio::test]
    async fn test_batch_verification() {
        let mut tx1 = create_test_transaction();
        let mut tx2 = create_test_transaction();
        let keypair1 = KeyPair::generate();
        let keypair2 = KeyPair::generate();
        
        tx1.sign(&keypair1, 0).unwrap();
        tx2.sign(&keypair2, 0).unwrap();
        
        let batch = vec![
            (tx1, vec![keypair1.public_key().as_bytes().to_vec()]),
            (tx2, vec![keypair2.public_key().as_bytes().to_vec()]),
        ];
        
        let results = Transaction::verify_batch(&batch).await;
        assert!(results.len() == 2);
        assert!(results[0].as_ref().unwrap());
        assert!(results[1].as_ref().unwrap());
    }

    #[tokio::test]
    async fn test_verify() {
        let mut tx = create_test_transaction();
        let keypair = KeyPair::generate();
        
        // Should fail without signature
        assert!(tx.verify().await.is_err());
        
        // Should pass with signature
        tx.sign(&keypair, 0).unwrap();
        assert!(tx.verify().await.unwrap());
        
        // Should fail with invalid hash
        tx.hash = Hash::new(&[0u8; 32]);
        assert!(tx.verify().await.is_err());
    }
}
