use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use crate::contract::{ContractError, ContractResult};

// State size limits
const MAX_STATE_SIZE: usize = 100 * 1024 * 1024; // 100MB total state size
const MAX_KEY_SIZE: usize = 1024; // 1KB max key size
const MAX_VALUE_SIZE: usize = 1024 * 1024; // 1MB max value size
const MAX_ENTRIES: usize = 100_000; // Maximum number of key-value pairs

/// Represents a snapshot of contract state at a specific point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Contract address this snapshot belongs to
    pub contract_addr: [u8; 32],
    /// Version of the contract when snapshot was taken
    pub version: String,
    /// Timestamp when snapshot was created
    pub timestamp: u64,
    /// The actual state data
    pub state: HashMap<Vec<u8>, Vec<u8>>,
    /// Hash of the state for integrity verification
    pub state_hash: [u8; 32],
    /// Schema version of the state
    pub schema_version: u32,
}

/// Tracks changes between states for efficient updates and rollbacks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDiff {
    /// Keys that were added in this diff
    pub added: HashMap<Vec<u8>, Vec<u8>>,
    /// Keys that were modified in this diff
    pub modified: HashMap<Vec<u8>, (Vec<u8>, Vec<u8>)>, // (old_value, new_value)
    /// Keys that were deleted in this diff
    pub deleted: HashMap<Vec<u8>, Vec<u8>>,
}

/// Manages contract state including snapshots and migrations
#[derive(Debug)]
pub struct StateManager {
    /// Current state for each contract
    states: HashMap<[u8; 32], HashMap<Vec<u8>, Vec<u8>>>,
    /// History of state snapshots
    snapshots: HashMap<[u8; 32], Vec<StateSnapshot>>,
    /// Track state changes for each contract
    diffs: HashMap<[u8; 32], Vec<StateDiff>>,
}

impl StateManager {
    pub fn new() -> Self {
        StateManager {
            states: HashMap::new(),
            snapshots: HashMap::new(),
            diffs: HashMap::new(),
        }
    }

    /// Calculate total state size for a contract
    fn calculate_state_size(state: &HashMap<Vec<u8>, Vec<u8>>) -> usize {
        state.iter().map(|(k, v)| k.len() + v.len()).sum()
    }

    /// Validate state update against size limits
    fn validate_state_update(
        &self,
        state: &HashMap<Vec<u8>, Vec<u8>>,
        new_key: &[u8],
        new_value: &[u8]
    ) -> ContractResult<()> {
        // Check key size
        if new_key.len() > MAX_KEY_SIZE {
            return Err(ContractError::StateError(
                format!("Key size {} exceeds maximum allowed size of {} bytes", 
                    new_key.len(), MAX_KEY_SIZE)
            ));
        }

        // Check value size
        if new_value.len() > MAX_VALUE_SIZE {
            return Err(ContractError::StateError(
                format!("Value size {} exceeds maximum allowed size of {} bytes", 
                    new_value.len(), MAX_VALUE_SIZE)
            ));
        }

        // Calculate new total size
        let mut total_size = Self::calculate_state_size(state);
        if let Some(existing_value) = state.get(new_key) {
            total_size -= new_key.len() + existing_value.len();
        }
        total_size += new_key.len() + new_value.len();

        // Check total state size
        if total_size > MAX_STATE_SIZE {
            return Err(ContractError::StateError(
                format!("Total state size {} would exceed maximum allowed size of {} bytes", 
                    total_size, MAX_STATE_SIZE)
            ));
        }

        // Check number of entries
        if !state.contains_key(new_key) && state.len() >= MAX_ENTRIES {
            return Err(ContractError::StateError(
                format!("Maximum number of entries ({}) exceeded", MAX_ENTRIES)
            ));
        }

        Ok(())
    }

    /// Create a snapshot of current contract state
    pub fn create_snapshot(&mut self, contract_addr: [u8; 32], version: String) -> ContractResult<StateSnapshot> {
        let state = self.states.get(&contract_addr).ok_or_else(|| {
            ContractError::StateError("Contract state not found".into())
        })?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Create state hash for integrity verification
        let state_hash = self.compute_state_hash(state);

        let snapshot = StateSnapshot {
            contract_addr,
            version,
            timestamp,
            state: state.clone(),
            state_hash,
            schema_version: 1, // Initialize with version 1
        };

        // Store the snapshot
        self.snapshots
            .entry(contract_addr)
            .or_insert_with(Vec::new)
            .push(snapshot.clone());

        Ok(snapshot)
    }

    /// Restore contract state from a snapshot
    pub fn restore_from_snapshot(&mut self, contract_addr: [u8; 32], timestamp: u64) -> ContractResult<()> {
        let snapshots = self.snapshots.get(&contract_addr).ok_or_else(|| {
            ContractError::StateError("No snapshots found for contract".into())
        })?;

        let snapshot = snapshots.iter().find(|s| s.timestamp == timestamp).ok_or_else(|| {
            ContractError::StateError("Snapshot not found for given timestamp".into())
        })?;

        // Verify state integrity
        if !self.verify_state_integrity(snapshot) {
            return Err(ContractError::StateError("State integrity verification failed".into()));
        }

        // Restore the state
        self.states.insert(contract_addr, snapshot.state.clone());

        Ok(())
    }

    /// Track changes between old and new state
    pub fn track_state_changes(&mut self, contract_addr: [u8; 32], old_state: &HashMap<Vec<u8>, Vec<u8>>, new_state: &HashMap<Vec<u8>, Vec<u8>>) {
        let mut diff = StateDiff {
            added: HashMap::new(),
            modified: HashMap::new(),
            deleted: HashMap::new(),
        };

        // Find added and modified keys
        for (key, new_value) in new_state {
            match old_state.get(key) {
                Some(old_value) if old_value != new_value => {
                    diff.modified.insert(key.clone(), (old_value.clone(), new_value.clone()));
                }
                None => {
                    diff.added.insert(key.clone(), new_value.clone());
                }
                _ => {}
            }
        }

        // Find deleted keys
        for key in old_state.keys() {
            if !new_state.contains_key(key) {
                diff.deleted.insert(key.clone(), old_state[key].clone());
            }
        }

        // Store the diff
        self.diffs.entry(contract_addr)
            .or_insert_with(Vec::new)
            .push(diff);
    }

    /// Compute hash of state for integrity verification
    fn compute_state_hash(&self, state: &HashMap<Vec<u8>, Vec<u8>>) -> [u8; 32] {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        
        // Sort keys for consistent hashing
        let mut keys: Vec<_> = state.keys().collect();
        keys.sort();

        for key in keys {
            hasher.update(key);
            hasher.update(&state[key]);
        }

        hasher.finalize().into()
    }

    /// Verify integrity of a state snapshot
    fn verify_state_integrity(&self, snapshot: &StateSnapshot) -> bool {
        let computed_hash = self.compute_state_hash(&snapshot.state);
        computed_hash == snapshot.state_hash
    }

    /// Get current state for a contract
    pub fn get_state(&self, contract_addr: &[u8; 32]) -> Option<&HashMap<Vec<u8>, Vec<u8>>> {
        self.states.get(contract_addr)
    }

    /// Update state for a contract
    pub fn update_state(&mut self, contract_addr: [u8; 32], key: Vec<u8>, value: Vec<u8>) -> ContractResult<()> {
        let old_state = self.states.get(&contract_addr).cloned().unwrap_or_default();
        
        // Validate state update against size limits
        self.validate_state_update(&old_state, &key, &value)?;

        // Create new state with update
        let mut new_state = old_state.clone();
        new_state.insert(key, value);

        // Track changes
        self.track_state_changes(contract_addr, &old_state, &new_state);

        // Update state
        self.states.insert(contract_addr, new_state);

        Ok(())
    }

    /// Get state diff history for a contract
    pub fn get_state_diffs(&self, contract_addr: &[u8; 32]) -> Option<&Vec<StateDiff>> {
        self.diffs.get(contract_addr)
    }

    /// Get snapshot history for a contract
    pub fn get_snapshots(&self, contract_addr: &[u8; 32]) -> Option<&Vec<StateSnapshot>> {
        self.snapshots.get(contract_addr)
    }

    /// Get current state size for a contract
    pub fn get_state_size(&self, contract_addr: &[u8; 32]) -> usize {
        self.states
            .get(contract_addr)
            .map_or(0, Self::calculate_state_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_snapshot_creation() {
        let mut manager = StateManager::new();
        let contract_addr = [0u8; 32];
        let version = "1.0.0".to_string();

        // Initialize some state
        manager.states.insert(contract_addr, {
            let mut state = HashMap::new();
            state.insert(b"key1".to_vec(), b"value1".to_vec());
            state
        });

        let snapshot = manager.create_snapshot(contract_addr, version).unwrap();
        
        assert_eq!(snapshot.contract_addr, contract_addr);
        assert_eq!(snapshot.version, "1.0.0");
        assert_eq!(snapshot.state.len(), 1);
        assert_eq!(snapshot.schema_version, 1);
    }

    #[test]
    fn test_state_restoration() {
        let mut manager = StateManager::new();
        let contract_addr = [0u8; 32];
        let version = "1.0.0".to_string();

        // Initialize some state
        manager.states.insert(contract_addr, {
            let mut state = HashMap::new();
            state.insert(b"key1".to_vec(), b"value1".to_vec());
            state
        });

        // Create snapshot
        let snapshot = manager.create_snapshot(contract_addr, version).unwrap();

        // Modify state
        manager.update_state(contract_addr, b"key1".to_vec(), b"value2".to_vec()).unwrap();

        // Restore from snapshot
        manager.restore_from_snapshot(contract_addr, snapshot.timestamp).unwrap();

        // Verify state was restored
        let restored_state = manager.get_state(&contract_addr).unwrap();
        assert_eq!(restored_state.get(&b"key1".to_vec()).unwrap(), &b"value1".to_vec());
    }

    #[test]
    fn test_state_diff_tracking() {
        let mut manager = StateManager::new();
        let contract_addr = [0u8; 32];

        // Initialize state
        let mut old_state = HashMap::new();
        old_state.insert(b"key1".to_vec(), b"value1".to_vec());
        old_state.insert(b"key2".to_vec(), b"value2".to_vec());

        let mut new_state = old_state.clone();
        new_state.insert(b"key1".to_vec(), b"value1_modified".to_vec()); // Modified
        new_state.insert(b"key3".to_vec(), b"value3".to_vec()); // Added
        new_state.remove(&b"key2".to_vec()); // Deleted

        manager.track_state_changes(contract_addr, &old_state, &new_state);

        let diffs = manager.get_state_diffs(&contract_addr).unwrap();
        assert_eq!(diffs.len(), 1);

        let diff = &diffs[0];
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.modified.len(), 1);
        assert_eq!(diff.deleted.len(), 1);
    }

    #[test]
    fn test_state_size_limits() {
        let mut manager = StateManager::new();
        let contract_addr = [0u8; 32];

        // Test key size limit
        let large_key = vec![0u8; MAX_KEY_SIZE + 1];
        let result = manager.update_state(contract_addr, large_key, vec![1]);
        assert!(result.is_err());

        // Test value size limit
        let large_value = vec![0u8; MAX_VALUE_SIZE + 1];
        let result = manager.update_state(contract_addr, vec![1], large_value);
        assert!(result.is_err());

        // Test total state size limit
        let value_size = MAX_STATE_SIZE / 10;
        for i in 0..11 {
            let key = vec![i as u8];
            let value = vec![0u8; value_size];
            let result = manager.update_state(contract_addr, key, value);
            if i < 10 {
                assert!(result.is_ok());
            } else {
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn test_max_entries_limit() {
        let mut manager = StateManager::new();
        let contract_addr = [0u8; 32];

        // Add maximum allowed entries
        for i in 0..MAX_ENTRIES {
            let key = format!("key{}", i).into_bytes();
            let value = vec![1];
            let result = manager.update_state(contract_addr, key, value);
            assert!(result.is_ok());
        }

        // Try to add one more entry
        let result = manager.update_state(
            contract_addr,
            format!("key{}", MAX_ENTRIES).into_bytes(),
            vec![1]
        );
        assert!(result.is_err());
    }
}
