use rocksdb::{DB, Options, BlockBasedOptions, WriteOptions, ReadOptions, CompactOptions, SliceTransform};
use std::path::Path;
use std::sync::Arc;
use crate::block::{Block, BlockHeader};
use crate::transaction::Transaction;
use crate::crypto::Hash;
use bincode;
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

// Column family names
const BLOCKS_CF: &str = "blocks";
const TRANSACTIONS_CF: &str = "transactions";
const UTXOS_CF: &str = "utxos";
const STATE_CF: &str = "state";
const METADATA_CF: &str = "metadata";
const CONTRACT_CF: &str = "contracts";

#[derive(Debug)]
pub enum StorageError {
    DatabaseError(String),
    SerializationError(String),
    NotFound,
    InvalidData,
    CacheError(String),
}

// Simple in-memory storage for testing
pub struct Storage {
    data: std::collections::HashMap<Vec<u8>, Vec<u8>>,
}

impl Storage {
    pub fn new_in_memory() -> Result<Self, StorageError> {
        Ok(Storage {
            data: std::collections::HashMap::new(),
        })
    }

    pub fn set(&mut self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        self.data.insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.data.get(key).cloned())
    }

    pub fn delete(&mut self, key: &[u8]) -> Result<(), StorageError> {
        self.data.remove(key);
        Ok(())
    }
}

impl From<rocksdb::Error> for StorageError {
    fn from(err: rocksdb::Error) -> Self {
        StorageError::DatabaseError(err.to_string())
    }
}

impl From<Box<dyn Error>> for StorageError {
    fn from(err: Box<dyn Error>) -> Self {
        StorageError::DatabaseError(err.to_string())
    }
}

pub struct BlockchainDB {
    db: DB,
    write_options: WriteOptions,
    read_options: ReadOptions,
}

impl BlockchainDB {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_max_background_jobs(4);
        opts.set_max_background_compactions(2);
        opts.set_max_background_flushes(2);
        opts.set_keep_log_file_num(10);
        opts.set_max_log_file_size(1024 * 1024 * 10); // 10MB
        opts.set_log_level(rocksdb::LogLevel::Info);
        
        // Configure block cache
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_block_size(16 * 1024); // 16KB
        block_opts.set_cache_index_and_filter_blocks(true);
        block_opts.set_pin_l0_filter_and_index_blocks_in_cache(true);
        block_opts.set_format_version(5);
        opts.set_block_based_table_factory(&block_opts);

        // Enable compression
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        opts.set_bottommost_compression_type(rocksdb::DBCompressionType::Zstd);

        // Configure prefix extractor for efficient queries
        opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(32)); // Hash size

        let column_families = vec![BLOCKS_CF, TRANSACTIONS_CF, UTXOS_CF, STATE_CF, METADATA_CF, CONTRACT_CF];
        let db = DB::open_cf(&opts, path, &column_families)?;

        let mut write_options = WriteOptions::default();
        write_options.set_sync(true);
        
        let mut read_options = ReadOptions::default();
        read_options.set_verify_checksums(true);
        read_options.set_readahead_size(1024 * 1024); // 1MB readahead

        Ok(BlockchainDB { 
            db,
            write_options,
            read_options,
        })
    }

    pub async fn store_block(&self, block: &Block) -> Result<(), StorageError> {
        let cf = self.db.cf_handle(BLOCKS_CF)
            .ok_or(StorageError::DatabaseError("Block CF not found".to_string()))?;
        
        let key = block.hash.to_bytes();
        let value = bincode::serialize(block)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        
        self.db.put_cf_opt(cf, key, value, &self.write_options)?;
        
        // Update metadata
        self.update_metadata(&block.hash)?;
        
        Ok(())
    }

    pub async fn get_block(&self, hash: &Hash) -> Result<Block, StorageError> {
        let cf = self.db.cf_handle(BLOCKS_CF)
            .ok_or(StorageError::DatabaseError("Block CF not found".to_string()))?;
        
        if let Some(data) = self.db.get_cf_opt(cf, hash.to_bytes(), &self.read_options)? {
            let block: Block = bincode::deserialize(&data)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;
            Ok(block)
        } else {
            Err(StorageError::NotFound)
        }
    }

    pub async fn store_transaction(&self, tx: &Transaction) -> Result<(), StorageError> {
        let cf = self.db.cf_handle(TRANSACTIONS_CF)
            .ok_or(StorageError::DatabaseError("Transaction CF not found".to_string()))?;
        
        let key = tx.hash.to_bytes();
        let value = bincode::serialize(tx)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        
        self.db.put_cf_opt(cf, key, value, &self.write_options)?;
        Ok(())
    }

    pub async fn get_transaction(&self, hash: &Hash) -> Result<Transaction, StorageError> {
        let cf = self.db.cf_handle(TRANSACTIONS_CF)
            .ok_or(StorageError::DatabaseError("Transaction CF not found".to_string()))?;
        
        if let Some(data) = self.db.get_cf_opt(cf, hash.to_bytes(), &self.read_options)? {
            let tx: Transaction = bincode::deserialize(&data)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;
            Ok(tx)
        } else {
            Err(StorageError::NotFound)
        }
    }

    pub async fn optimize_storage(&mut self) -> Result<(), StorageError> {
        // Trigger compaction for all column families
        for cf_name in &[BLOCKS_CF, TRANSACTIONS_CF, UTXOS_CF, STATE_CF, METADATA_CF, CONTRACT_CF] {
            if let Some(cf) = self.db.cf_handle(cf_name) {
                let mut compact_opts = CompactOptions::default();
                compact_opts.set_exclusive_manual_compaction(true);
                self.db.compact_range_cf_opt(cf, None::<&[u8]>, None::<&[u8]>, &compact_opts);
            }
        }

        // Update optimization metadata
        let cf = self.db.cf_handle(METADATA_CF)
            .ok_or(StorageError::DatabaseError("Metadata CF not found".to_string()))?;
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        self.db.put_cf_opt(
            cf,
            b"last_optimization",
            timestamp.to_string().as_bytes(),
            &self.write_options
        )?;

        Ok(())
    }

    fn update_metadata(&self, latest_block_hash: &Hash) -> Result<(), StorageError> {
        let cf = self.db.cf_handle(METADATA_CF)
            .ok_or(StorageError::DatabaseError("Metadata CF not found".to_string()))?;
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Store latest block hash
        self.db.put_cf_opt(
            cf,
            b"latest_block",
            latest_block_hash.to_bytes(),
            &self.write_options
        )?;
        
        // Store last update timestamp
        self.db.put_cf_opt(
            cf,
            b"last_update",
            timestamp.to_string().as_bytes(),
            &self.write_options
        )?;

        Ok(())
    }

    pub async fn get_storage_stats(&self) -> Result<String, StorageError> {
        let mut stats = String::new();
        
        // Get statistics for each column family
        for cf_name in &[BLOCKS_CF, TRANSACTIONS_CF, UTXOS_CF, STATE_CF, METADATA_CF, CONTRACT_CF] {
            if let Some(cf) = self.db.cf_handle(cf_name) {
                let cf_stats = self.db.property_value_cf(cf, "rocksdb.stats")?
                    .ok_or(StorageError::DatabaseError("Could not get CF stats".to_string()))?;
                stats.push_str(&format!("\n=== {} Statistics ===\n{}", cf_name, cf_stats));
            }
        }

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio;

    #[tokio::test]
    async fn test_blockchain_db() -> Result<(), StorageError> {
        let temp_dir = tempdir().map_err(|e| StorageError::DatabaseError(e.to_string()))?;
        let db = BlockchainDB::new(temp_dir.path())?;

        // Create a test block
        let block = Block {
            header: BlockHeader {
                version: 1,
                timestamp: 12345,
                prev_hash: Hash::new(b"previous hash"),
                merkle_root: Hash::new(b"merkle root"),
                difficulty: 1,
                nonce: 0,
            },
            transactions: vec![],
            hash: Hash::new(b"block hash"),
        };

        // Test block storage and retrieval
        db.store_block(&block).await?;
        let retrieved_block = db.get_block(&block.hash).await?;
        assert_eq!(retrieved_block.hash.to_bytes(), block.hash.to_bytes());

        // Test storage optimization
        let mut db = db;
        db.optimize_storage().await?;

        // Test storage statistics
        let stats = db.get_storage_stats().await?;
        assert!(!stats.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_in_memory_storage() -> Result<(), StorageError> {
        let mut storage = Storage::new_in_memory()?;
        
        // Test basic operations
        storage.set(b"test_key", b"test_value")?;
        let value = storage.get(b"test_key")?;
        assert_eq!(value, Some(b"test_value".to_vec()));
        
        storage.delete(b"test_key")?;
        let value = storage.get(b"test_key")?;
        assert_eq!(value, None);
        
        Ok(())
    }
}
