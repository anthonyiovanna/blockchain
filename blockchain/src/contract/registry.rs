use std::collections::{HashMap, BTreeMap};
use serde::{Serialize, Deserialize};
use super::{ContractMetadata, ContractVersion, ContractResult, ContractError};

/// Registry index types for efficient contract lookup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegistryIndex {
    Version(String),
    Author([u8; 32]),
    CreationTime(u64),
    UpdateTime(u64),
    Description(String),
}

/// Tracks the upgrade history of a contract
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpgradeHistory {
    from_version: String,
    to_version: String,
    timestamp: u64,
    successful: bool,
    rollback_performed: bool,
}

/// Contract registry for efficient contract lookup and management
#[derive(Debug)]
pub struct ContractRegistry {
    // Main storage of contract versions by address
    versions: HashMap<[u8; 32], Vec<ContractVersion>>,
    
    // Indexes for efficient lookup
    version_index: BTreeMap<String, Vec<[u8; 32]>>,
    author_index: HashMap<[u8; 32], Vec<[u8; 32]>>,
    creation_time_index: BTreeMap<u64, Vec<[u8; 32]>>,
    update_time_index: BTreeMap<u64, Vec<[u8; 32]>>,

    // Upgrade history for rollback support
    upgrade_history: HashMap<[u8; 32], Vec<UpgradeHistory>>,
}

impl ContractRegistry {
    pub fn new() -> Self {
        ContractRegistry {
            versions: HashMap::new(),
            version_index: BTreeMap::new(),
            author_index: HashMap::new(),
            creation_time_index: BTreeMap::new(),
            update_time_index: BTreeMap::new(),
            upgrade_history: HashMap::new(),
        }
    }

    /// Verify bytecode integrity
    fn verify_bytecode(&self, bytecode: &[u8]) -> ContractResult<()> {
        if bytecode.is_empty() {
            return Err(ContractError::BytecodeVerificationError(
                "Empty bytecode provided".into()
            ));
        }

        // Add more bytecode verification logic here
        // For example, check magic numbers, format, etc.

        Ok(())
    }

    /// Check version compatibility
    fn check_version_compatibility(&self, address: &[u8; 32], new_version: &ContractVersion) -> ContractResult<()> {
        if let Some(versions) = self.versions.get(address) {
            if let Some(latest) = versions.last() {
                // Check semantic versioning compatibility
                let current_version = semver::Version::parse(&latest.metadata.version)
                    .map_err(|_| ContractError::VersionIncompatible("Invalid current version format".into()))?;
                
                let new_ver = semver::Version::parse(&new_version.metadata.version)
                    .map_err(|_| ContractError::VersionIncompatible("Invalid new version format".into()))?;

                if new_ver <= current_version {
                    return Err(ContractError::VersionConflict(
                        format!("New version {} must be greater than current version {}", 
                            new_version.metadata.version, latest.metadata.version)
                    ));
                }

                // Check if upgrade is allowed
                if !latest.metadata.is_upgradeable {
                    return Err(ContractError::UpgradeValidationError(
                        "Current contract version is not upgradeable".into()
                    ));
                }
            }
        }
        Ok(())
    }

    /// Validate contract state during upgrade
    fn validate_contract_state(&self, address: &[u8; 32], new_version: &ContractVersion) -> ContractResult<()> {
        // Add state validation logic here
        // For example, check state variables compatibility, storage layout, etc.
        Ok(())
    }

    /// Register a new contract version with validation
    pub fn register_version(&mut self, address: [u8; 32], version: ContractVersion) -> ContractResult<()> {
        // Verify bytecode
        self.verify_bytecode(&version.bytecode)?;

        // Check version compatibility
        self.check_version_compatibility(&address, &version)?;

        // Validate contract state if this is an upgrade
        if self.versions.contains_key(&address) {
            self.validate_contract_state(&address, &version)?;
        }

        // Store previous version for potential rollback
        let previous_version = if let Some(versions) = self.versions.get(&address) {
            versions.last().cloned()
        } else {
            None
        };

        // Add to main storage
        self.versions
            .entry(address)
            .or_insert_with(Vec::new)
            .push(version.clone());

        // Update indexes
        self.version_index
            .entry(version.metadata.version.clone())
            .or_insert_with(Vec::new)
            .push(address);

        self.author_index
            .entry(version.metadata.author)
            .or_insert_with(Vec::new)
            .push(address);

        self.creation_time_index
            .entry(version.metadata.created_at)
            .or_insert_with(Vec::new)
            .push(address);

        self.update_time_index
            .entry(version.metadata.updated_at)
            .or_insert_with(Vec::new)
            .push(address);

        // Record upgrade history if this is an upgrade
        if let Some(prev) = previous_version {
            let history = UpgradeHistory {
                from_version: prev.metadata.version,
                to_version: version.metadata.version,
                timestamp: version.metadata.updated_at,
                successful: true,
                rollback_performed: false,
            };
            self.upgrade_history
                .entry(address)
                .or_insert_with(Vec::new)
                .push(history);
        }

        Ok(())
    }

    /// Rollback to previous version
    pub fn rollback_version(&mut self, address: [u8; 32]) -> ContractResult<()> {
        let versions = self.versions.get_mut(&address)
            .ok_or_else(|| ContractError::NotFound("Contract not found".into()))?;

        if versions.len() < 2 {
            return Err(ContractError::StateRollbackFailed(
                "No previous version available for rollback".into()
            ));
        }

        // Remove latest version
        versions.pop();

        // Update upgrade history
        if let Some(history) = self.upgrade_history.get_mut(&address) {
            if let Some(last_upgrade) = history.last_mut() {
                last_upgrade.rollback_performed = true;
                last_upgrade.successful = false;
            }
        }

        Ok(())
    }

    /// Find contracts by metadata field with enhanced error handling
    pub fn find_by_index(&self, index: RegistryIndex) -> ContractResult<Vec<([u8; 32], &ContractVersion)>> {
        let addresses = match &index {
            RegistryIndex::Version(version) => {
                self.version_index.get(version)
                    .ok_or_else(|| ContractError::VersionNotFound(
                        format!("No contracts found for version {}", version)
                    ))?
            },
            RegistryIndex::Author(author) => {
                self.author_index.get(author)
                    .ok_or_else(|| ContractError::NotFound(
                        format!("No contracts found for author {:?}", author)
                    ))?
            },
            RegistryIndex::CreationTime(time) => {
                self.creation_time_index.get(time)
                    .ok_or_else(|| ContractError::NotFound(
                        format!("No contracts found for creation time {}", time)
                    ))?
            },
            RegistryIndex::UpdateTime(time) => {
                self.update_time_index.get(time)
                    .ok_or_else(|| ContractError::NotFound(
                        format!("No contracts found for update time {}", time)
                    ))?
            },
            RegistryIndex::Description(desc) => {
                return Ok(self.versions.iter()
                    .filter_map(|(addr, versions)| {
                        versions.last().and_then(|v| {
                            if v.metadata.description.contains(desc) {
                                Some((*addr, v))
                            } else {
                                None
                            }
                        })
                    })
                    .collect());
            }
        };

        let mut results = Vec::new();
        for addr in addresses {
            if let Some(versions) = self.versions.get(addr) {
                if let Some(latest) = versions.last() {
                    results.push((*addr, latest));
                }
            }
        }

        Ok(results)
    }

    /// Get all versions of a contract with enhanced error context
    pub fn get_contract_versions(&self, address: &[u8; 32]) -> ContractResult<&Vec<ContractVersion>> {
        self.versions
            .get(address)
            .ok_or_else(|| ContractError::NotFound(
                format!("Contract not found at address {:?}", address)
            ))
    }

    /// Get specific version of a contract with detailed error handling
    pub fn get_contract_version(&self, address: &[u8; 32], version: &str) -> ContractResult<&ContractVersion> {
        let versions = self.get_contract_versions(address)?;
        versions
            .iter()
            .find(|v| v.metadata.version == version)
            .ok_or_else(|| ContractError::VersionNotFound(
                format!("Version {} not found for contract {:?}", version, address)
            ))
    }

    /// Get latest version of a contract with enhanced error context
    pub fn get_latest_version(&self, address: &[u8; 32]) -> ContractResult<&ContractVersion> {
        let versions = self.get_contract_versions(address)?;
        versions
            .last()
            .ok_or_else(|| ContractError::NotFound(
                format!("No versions found for contract {:?}", address)
            ))
    }

    /// Get upgrade history for a contract
    pub fn get_upgrade_history(&self, address: &[u8; 32]) -> ContractResult<&Vec<UpgradeHistory>> {
        self.upgrade_history
            .get(address)
            .ok_or_else(|| ContractError::NotFound(
                format!("No upgrade history found for contract {:?}", address)
            ))
    }

    /// List all contracts with their latest versions
    pub fn list_all_contracts(&self) -> Vec<([u8; 32], &ContractVersion)> {
        self.versions
            .iter()
            .filter_map(|(addr, versions)| {
                versions.last().map(|v| (*addr, v))
            })
            .collect()
    }

    /// Search contracts by partial description
    pub fn search_by_description(&self, description: &str) -> Vec<([u8; 32], &ContractVersion)> {
        self.versions
            .iter()
            .filter_map(|(addr, versions)| {
                versions.last().and_then(|v| {
                    if v.metadata.description.to_lowercase().contains(&description.to_lowercase()) {
                        Some((*addr, v))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ContractABI, ContractMetadata};

    fn create_test_version(version: &str, author: [u8; 32], time: u64) -> ContractVersion {
        ContractVersion {
            bytecode: vec![1, 2, 3], // Non-empty bytecode for verification
            metadata: ContractMetadata {
                version: version.to_string(),
                created_at: time,
                updated_at: time,
                author,
                description: format!("Test contract version {}", version),
                is_upgradeable: true,
            },
            abi: ContractABI {
                methods: vec![],
                events: vec![],
                standards: vec![],
            },
        }
    }

    #[test]
    fn test_register_and_retrieve() {
        let mut registry = ContractRegistry::new();
        let address = [1u8; 32];
        let author = [2u8; 32];
        let version = create_test_version("1.0.0", author, 1000);

        registry.register_version(address, version.clone()).unwrap();

        let retrieved = registry.get_latest_version(&address).unwrap();
        assert_eq!(retrieved.metadata.version, "1.0.0");
    }

    #[test]
    fn test_version_compatibility() {
        let mut registry = ContractRegistry::new();
        let address = [1u8; 32];
        let author = [2u8; 32];
        
        // Register initial version
        let version1 = create_test_version("1.0.0", author, 1000);
        registry.register_version(address, version1).unwrap();

        // Try to register older version - should fail
        let version2 = create_test_version("0.9.0", author, 1001);
        assert!(registry.register_version(address, version2).is_err());

        // Register newer version - should succeed
        let version3 = create_test_version("1.1.0", author, 1002);
        assert!(registry.register_version(address, version3).is_ok());
    }

    #[test]
    fn test_rollback() {
        let mut registry = ContractRegistry::new();
        let address = [1u8; 32];
        let author = [2u8; 32];
        
        // Register two versions
        let version1 = create_test_version("1.0.0", author, 1000);
        let version2 = create_test_version("1.1.0", author, 1001);
        
        registry.register_version(address, version1).unwrap();
        registry.register_version(address, version2).unwrap();

        // Rollback to previous version
        registry.rollback_version(address).unwrap();

        // Check current version is 1.0.0
        let current = registry.get_latest_version(&address).unwrap();
        assert_eq!(current.metadata.version, "1.0.0");
    }
}
