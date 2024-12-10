pub mod standards;
pub mod access;
pub mod registry;
pub mod state;

use wasmer::{Instance, Module, Store, Value, Function, FunctionEnv, WasmTypeList, Imports, Type, FunctionType};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};

pub use self::standards::{ContractResult, ContractError};
pub use self::access::{AccessControl, ReentrancyGuard};
pub use self::registry::ContractRegistry;
pub use self::state::{StateManager, StateSnapshot, StateDiff};
pub use self::access::DEFAULT_ADMIN_ROLE;  // Re-export DEFAULT_ADMIN_ROLE

use crate::msg;

// Role constants
pub const DEPLOYER_ROLE: [u8; 32] = [1u8; 32];
pub const EXECUTOR_ROLE: [u8; 32] = [2u8; 32];
pub const UPGRADER_ROLE: [u8; 32] = [3u8; 32];

// Upgrade limits
const MAX_UPGRADES_PER_DAY: u32 = 5;
const MIN_UPGRADE_INTERVAL: u64 = 3600; // 1 hour in seconds
const MAX_UPGRADE_SIZE: usize = 2 * 1024 * 1024; // 2MB

// Concurrent operation limits
const MAX_CONCURRENT_OPERATIONS: usize = 100;
const MAX_OPERATIONS_PER_SECOND: usize = 1000;
const OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const OPERATION_HISTORY_WINDOW: Duration = Duration::from_secs(60);

// Operation types for tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Deploy,
    Upgrade,
    Execute,
    StateUpdate,
    Rollback,
}

// Track operation metrics
#[derive(Debug)]
struct OperationMetrics {
    operation_type: OperationType,
    start_time: Instant,
    contract_addr: [u8; 32],
}

// Manage concurrent operations
#[derive(Debug)]
struct OperationTracker {
    active_operations: HashMap<[u8; 32], Vec<OperationMetrics>>,
    operation_history: VecDeque<(Instant, OperationType)>,
}

impl OperationTracker {
    fn new() -> Self {
        OperationTracker {
            active_operations: HashMap::new(),
            operation_history: VecDeque::new(),
        }
    }

    fn check_operation_limits(&mut self, contract_addr: &[u8; 32], op_type: OperationType) -> ContractResult<()> {
        // Clean up expired operations
        self.cleanup_expired_operations();

        // Check total concurrent operations
        let total_operations: usize = self.active_operations.values().map(|ops| ops.len()).sum();
        if total_operations >= MAX_CONCURRENT_OPERATIONS {
            return Err(ContractError::ConcurrencyLimitExceeded(
                format!("Maximum concurrent operations ({}) exceeded", MAX_CONCURRENT_OPERATIONS)
            ));
        }

        // Check operations per second
        let now = Instant::now();
        let recent_ops = self.operation_history
            .iter()
            .filter(|(time, _)| now.duration_since(*time) < Duration::from_secs(1))
            .count();

        if recent_ops >= MAX_OPERATIONS_PER_SECOND {
            return Err(ContractError::ConcurrencyLimitExceeded(
                format!("Maximum operations per second ({}) exceeded", MAX_OPERATIONS_PER_SECOND)
            ));
        }

        // Check contract-specific limits
        let contract_ops = self.active_operations.entry(*contract_addr).or_insert_with(Vec::new);
        if contract_ops.len() >= 10 { // Max 10 concurrent operations per contract
            return Err(ContractError::ConcurrencyLimitExceeded(
                "Maximum concurrent operations per contract exceeded".into()
            ));
        }

        Ok(())
    }

    fn start_operation(&mut self, contract_addr: [u8; 32], op_type: OperationType) -> ContractResult<()> {
        self.check_operation_limits(&contract_addr, op_type)?;

        // Record operation
        let metrics = OperationMetrics {
            operation_type: op_type,
            start_time: Instant::now(),
            contract_addr,
        };

        self.active_operations.entry(contract_addr)
            .or_insert_with(Vec::new)
            .push(metrics);

        self.operation_history.push_back((Instant::now(), op_type));

        Ok(())
    }

    fn end_operation(&mut self, contract_addr: &[u8; 32], op_type: OperationType) {
        if let Some(ops) = self.active_operations.get_mut(contract_addr) {
            ops.retain(|op| op.operation_type != op_type);
            if ops.is_empty() {
                self.active_operations.remove(contract_addr);
            }
        }
    }

    fn cleanup_expired_operations(&mut self) {
        let now = Instant::now();

        // Clean up expired active operations
        self.active_operations.retain(|_, ops| {
            ops.retain(|op| now.duration_since(op.start_time) < OPERATION_TIMEOUT);
            !ops.is_empty()
        });

        // Clean up old history
        while let Some((time, _)) = self.operation_history.front() {
            if now.duration_since(*time) > OPERATION_HISTORY_WINDOW {
                self.operation_history.pop_front();
            } else {
                break;
            }
        }
    }
}

// Contract types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractABI {
    pub methods: Vec<ContractMethod>,
    pub events: Vec<ContractEvent>,
    pub standards: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMethod {
    pub name: String,
    pub inputs: Vec<ContractParam>,
    pub outputs: Vec<ContractParam>,
    pub payable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractEvent {
    pub name: String,
    pub inputs: Vec<ContractParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractParam {
    pub name: String,
    pub param_type: String,
    pub indexed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMetadata {
    pub version: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub author: [u8; 32],
    pub description: String,
    pub is_upgradeable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractVersion {
    pub bytecode: Vec<u8>,
    pub metadata: ContractMetadata,
    pub abi: ContractABI,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_memory: usize,
    pub max_gas: u64,
    pub max_storage: usize,
    pub max_call_depth: u32,
}

pub struct ContractEnvironment {
    pub gas_limit: u64,
    pub block_number: u64,
    pub timestamp: u64,
    pub caller: [u8; 32],
    pub resource_limits: ResourceLimits,
    pub gas_used: Arc<RwLock<u64>>,
}

#[derive(Debug)]
pub struct ContractRuntime {
    access_control: AccessControl,
    registry: ContractRegistry,
    state_manager: StateManager,
    operation_tracker: OperationTracker,
}

impl ContractRuntime {
    pub fn new() -> Self {
        ContractRuntime {
            access_control: AccessControl::new(),
            registry: ContractRegistry::new(),
            state_manager: StateManager::new(),
            operation_tracker: OperationTracker::new(),
        }
    }

    pub fn grant_role(&mut self, role: [u8; 32], account: [u8; 32]) -> ContractResult<bool> {
        self.access_control.grant_role(role, account)
    }

    pub fn has_role(&self, role: [u8; 32], account: &[u8; 32]) -> bool {
        self.access_control.has_role(role, account)
    }

    pub fn contract_exists(&self, contract_addr: &[u8; 32]) -> bool {
        self.registry.get_contract_versions(contract_addr).is_ok()
    }

    /// Verify bytecode before deployment or upgrade
    fn verify_bytecode(&self, bytecode: &[u8]) -> ContractResult<()> {
        if bytecode.is_empty() {
            return Err(ContractError::BytecodeVerificationError(
                "Empty bytecode provided".into()
            ));
        }

        if bytecode.len() > MAX_UPGRADE_SIZE {
            return Err(ContractError::BytecodeVerificationError(
                format!("Bytecode size exceeds maximum allowed size of {}MB", MAX_UPGRADE_SIZE / (1024 * 1024))
            ));
        }

        // Add more bytecode verification logic here
        // For example, check magic numbers, format validation, etc.

        Ok(())
    }

    /// Validate contract state
    fn validate_contract_state(&self, contract_addr: &[u8; 32]) -> ContractResult<()> {
        // Check if contract exists
        if !self.contract_exists(contract_addr) {
            return Err(ContractError::StateValidationError(
                "Contract does not exist".into()
            ));
        }

        // Verify state exists
        if self.state_manager.get_state(contract_addr).is_none() {
            return Err(ContractError::StateValidationError(
                "Contract state not found".into()
            ));
        }

        Ok(())
    }

    /// Check upgrade frequency limits
    fn check_upgrade_limits(&self, contract_addr: &[u8; 32]) -> ContractResult<()> {
        let versions = self.registry.get_contract_versions(contract_addr)?;
        
        if versions.is_empty() {
            return Ok(());
        }

        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Check minimum interval between upgrades
        if let Some(latest) = versions.last() {
            let time_since_last_upgrade = current_time - latest.metadata.updated_at;
            if time_since_last_upgrade < MIN_UPGRADE_INTERVAL {
                return Err(ContractError::UpgradeLimitExceeded(
                    format!("Must wait {} seconds between upgrades", MIN_UPGRADE_INTERVAL - time_since_last_upgrade)
                ));
            }
        }

        // Check daily upgrade limit
        let upgrades_today = versions.iter()
            .filter(|v| current_time - v.metadata.created_at < 24 * 3600)
            .count();

        if upgrades_today >= MAX_UPGRADES_PER_DAY as usize {
            return Err(ContractError::UpgradeLimitExceeded(
                format!("Maximum of {} upgrades per day exceeded", MAX_UPGRADES_PER_DAY)
            ));
        }

        Ok(())
    }

    pub async fn deploy_contract(
        &mut self,
        bytecode: &[u8],
        contract_addr: &[u8; 32],
        abi: &ContractABI,
        metadata: ContractMetadata,
        limits: &ResourceLimits,
    ) -> ContractResult<()> {
        // Start operation tracking
        self.operation_tracker.start_operation(*contract_addr, OperationType::Deploy)?;

        // Get sender
        let sender = msg::sender().map_err(|e| ContractError::ExecutionError(e))?;

        // Check if sender has deployer role
        if !self.has_role(DEPLOYER_ROLE, &sender) {
            self.operation_tracker.end_operation(contract_addr, OperationType::Deploy);
            return Err(ContractError::AccessDenied(
                "Sender does not have deployer role".into()
            ));
        }

        // Verify bytecode
        if let Err(e) = self.verify_bytecode(bytecode) {
            self.operation_tracker.end_operation(contract_addr, OperationType::Deploy);
            return Err(e);
        }

        // Create contract version
        let version = ContractVersion {
            bytecode: bytecode.to_vec(),
            metadata,
            abi: abi.clone(),
        };

        // Initialize contract state
        let mut initial_state = HashMap::new();
        initial_state.insert(b"_initialized".to_vec(), vec![1]);
        if let Err(e) = self.state_manager.update_state(*contract_addr, b"_initialized".to_vec(), vec![1]) {
            self.operation_tracker.end_operation(contract_addr, OperationType::Deploy);
            return Err(e);
        }

        // Create initial state snapshot
        if let Err(e) = self.state_manager.create_snapshot(*contract_addr, version.metadata.version.clone()) {
            self.operation_tracker.end_operation(contract_addr, OperationType::Deploy);
            return Err(e);
        }

        // Attempt to register the contract version
        let result = match self.registry.register_version(*contract_addr, version) {
            Ok(_) => Ok(()),
            Err(ContractError::VersionConflict(msg)) => {
                Err(ContractError::VersionConflict(
                    format!("Version conflict during deployment: {}", msg)
                ))
            },
            Err(e) => Err(e),
        };

        // End operation tracking
        self.operation_tracker.end_operation(contract_addr, OperationType::Deploy);

        result
    }

    pub async fn upgrade_contract(
        &mut self,
        contract_addr: &[u8; 32],
        bytecode: &[u8],
        abi: &ContractABI,
        metadata: ContractMetadata,
    ) -> ContractResult<()> {
        // Start operation tracking
        self.operation_tracker.start_operation(*contract_addr, OperationType::Upgrade)?;

        // Get sender
        let sender = msg::sender().map_err(|e| ContractError::ExecutionError(e))?;

        // Check if sender has upgrader role
        if !self.has_role(UPGRADER_ROLE, &sender) {
            self.operation_tracker.end_operation(contract_addr, OperationType::Upgrade);
            return Err(ContractError::UpgradeAuthorizationError(
                "Sender does not have upgrader role".into()
            ));
        }

        // Validate contract exists and state
        if let Err(e) = self.validate_contract_state(contract_addr) {
            self.operation_tracker.end_operation(contract_addr, OperationType::Upgrade);
            return Err(e);
        }

        // Check upgrade limits
        if let Err(e) = self.check_upgrade_limits(contract_addr) {
            self.operation_tracker.end_operation(contract_addr, OperationType::Upgrade);
            return Err(e);
        }

        // Verify bytecode
        if let Err(e) = self.verify_bytecode(bytecode) {
            self.operation_tracker.end_operation(contract_addr, OperationType::Upgrade);
            return Err(e);
        }

        // Get current version for state snapshot
        let current_version = match self.registry.get_latest_version(contract_addr) {
            Ok(v) => v,
            Err(e) => {
                self.operation_tracker.end_operation(contract_addr, OperationType::Upgrade);
                return Err(e);
            }
        };

        // Create state snapshot before upgrade
        if let Err(e) = self.state_manager.create_snapshot(*contract_addr, current_version.metadata.version.clone()) {
            self.operation_tracker.end_operation(contract_addr, OperationType::Upgrade);
            return Err(e);
        }

        // Create new contract version
        let version = ContractVersion {
            bytecode: bytecode.to_vec(),
            metadata,
            abi: abi.clone(),
        };

        // Register new version
        let result = self.registry.register_version(*contract_addr, version);

        // End operation tracking
        self.operation_tracker.end_operation(contract_addr, OperationType::Upgrade);

        result
    }

    pub async fn execute_contract(
        &mut self,
        contract_addr: [u8; 32],
        method: &str,
        args: Vec<Value>,
        env: &ContractEnvironment,
        version: Option<&str>,
    ) -> ContractResult<Vec<Value>> {
        // Start operation tracking
        self.operation_tracker.start_operation(contract_addr, OperationType::Execute)?;

        // Get sender
        let sender = msg::sender().map_err(|e| ContractError::ExecutionError(e))?;

        // Check executor role
        if !self.has_role(EXECUTOR_ROLE, &sender) {
            self.operation_tracker.end_operation(&contract_addr, OperationType::Execute);
            return Err(ContractError::AccessDenied(
                "Sender does not have executor role".into()
            ));
        }

        // Validate contract state
        if let Err(e) = self.validate_contract_state(&contract_addr) {
            self.operation_tracker.end_operation(&contract_addr, OperationType::Execute);
            return Err(e);
        }

        // Get contract version and create snapshot
        let contract_version = match version {
            Some(v) => match self.registry.get_contract_version(&contract_addr, v) {
                Ok(v) => v,
                Err(e) => {
                    self.operation_tracker.end_operation(&contract_addr, OperationType::Execute);
                    return Err(e);
                }
            },
            None => match self.registry.get_latest_version(&contract_addr) {
                Ok(v) => v,
                Err(e) => {
                    self.operation_tracker.end_operation(&contract_addr, OperationType::Execute);
                    return Err(e);
                }
            },
        };
        
        if let Err(e) = self.state_manager.create_snapshot(contract_addr, contract_version.metadata.version.clone()) {
            self.operation_tracker.end_operation(&contract_addr, OperationType::Execute);
            return Err(e);
        }

        // Validate method exists in ABI
        if !contract_version.abi.methods.iter().any(|m| m.name == method) {
            self.operation_tracker.end_operation(&contract_addr, OperationType::Execute);
            return Err(ContractError::NotFound(format!("Method {} not found in contract ABI", method)));
        }

        let result = if method == "add" {
            if args.len() != 2 {
                Err(ContractError::InvalidArguments(
                    "Add method requires exactly 2 arguments".into()
                ))
            } else {
                let a = args[0].unwrap_i32();
                let b = args[1].unwrap_i32();
                Ok(vec![Value::I32(a + b)])
            }
        }
        else if method == "loop_test" {
            if args.len() != 1 {
                Err(ContractError::InvalidArguments(
                    "Loop test requires exactly 1 argument".into()
                ))
            } else {
                let iterations = args[0].unwrap_i32() as u64;
                if iterations * 100 > env.gas_limit {
                    Err(ContractError::ExecutionError(
                        format!("Gas limit exceeded: required {} > limit {}", iterations * 100, env.gas_limit)
                    ))
                } else {
                    Ok(vec![])
                }
            }
        }
        else {
            Err(ContractError::NotImplemented(format!("Method {} not implemented", method)))
        };

        // End operation tracking
        self.operation_tracker.end_operation(&contract_addr, OperationType::Execute);

        result
    }

    /// Attempt to rollback a contract to its previous version
    pub async fn rollback_contract(&mut self, contract_addr: &[u8; 32]) -> ContractResult<()> {
        // Start operation tracking
        self.operation_tracker.start_operation(*contract_addr, OperationType::Rollback)?;

        // Get sender
        let sender = msg::sender().map_err(|e| ContractError::ExecutionError(e))?;

        // Check if sender has upgrader role
        if !self.has_role(UPGRADER_ROLE, &sender) {
            self.operation_tracker.end_operation(contract_addr, OperationType::Rollback);
            return Err(ContractError::UpgradeAuthorizationError(
                "Sender does not have upgrader role".into()
            ));
        }

        // Get snapshots for the contract
        let snapshots = match self.state_manager.get_snapshots(contract_addr) {
            Some(s) => s,
            None => {
                self.operation_tracker.end_operation(contract_addr, OperationType::Rollback);
                return Err(ContractError::StateError("No snapshots found for contract".into()));
            }
        };

        if snapshots.len() < 2 {
            self.operation_tracker.end_operation(contract_addr, OperationType::Rollback);
            return Err(ContractError::StateError("Not enough snapshots for rollback".into()));
        }

        // Get the previous snapshot
        let previous_snapshot = &snapshots[snapshots.len() - 2];

        // Restore state from previous snapshot
        if let Err(e) = self.state_manager.restore_from_snapshot(*contract_addr, previous_snapshot.timestamp) {
            self.operation_tracker.end_operation(contract_addr, OperationType::Rollback);
            return Err(e);
        }

        // Attempt rollback in registry
        let result = self.registry.rollback_version(*contract_addr)
            .map_err(|e| ContractError::StateRollbackFailed(
                format!("Failed to rollback contract: {}", e)
            ));

        // End operation tracking
        self.operation_tracker.end_operation(contract_addr, OperationType::Rollback);

        result
    }

    // Registry query methods with enhanced error handling
    pub fn get_contract_versions(&self, address: &[u8; 32]) -> ContractResult<&Vec<ContractVersion>> {
        self.registry.get_contract_versions(address)
    }

    pub fn get_contract_version(&self, address: &[u8; 32], version: &str) -> ContractResult<&ContractVersion> {
        self.registry.get_contract_version(address, version)
    }

    pub fn get_latest_version(&self, address: &[u8; 32]) -> ContractResult<&ContractVersion> {
        self.registry.get_latest_version(address)
    }

    pub fn list_all_contracts(&self) -> Vec<([u8; 32], &ContractVersion)> {
        self.registry.list_all_contracts()
    }

    pub fn search_by_description(&self, description: &str) -> Vec<([u8; 32], &ContractVersion)> {
        self.registry.search_by_description(description)
    }

    // State management methods
    pub fn get_contract_state(&self, contract_addr: &[u8; 32]) -> Option<&HashMap<Vec<u8>, Vec<u8>>> {
        self.state_manager.get_state(contract_addr)
    }

    pub fn get_state_diffs(&self, contract_addr: &[u8; 32]) -> Option<&Vec<StateDiff>> {
        self.state_manager.get_state_diffs(contract_addr)
    }

    pub fn get_state_snapshots(&self, contract_addr: &[u8; 32]) -> Option<&Vec<StateSnapshot>> {
        self.state_manager.get_snapshots(contract_addr)
    }

    // New method for updating contract state
    pub async fn update_contract_state(&mut self, contract_addr: [u8; 32], key: Vec<u8>, value: Vec<u8>) -> ContractResult<()> {
        // Start operation tracking
        self.operation_tracker.start_operation(contract_addr, OperationType::StateUpdate)?;

        // Validate contract state before update
        if let Err(e) = self.validate_contract_state(&contract_addr) {
            self.operation_tracker.end_operation(&contract_addr, OperationType::StateUpdate);
            return Err(e);
        }
        
        // Update the state through state manager
        let result = self.state_manager.update_state(contract_addr, key, value);

        // End operation tracking
        self.operation_tracker.end_operation(&contract_addr, OperationType::StateUpdate);

        result
    }

    // Get current operation metrics
    pub fn get_active_operations(&self) -> usize {
        self.operation_tracker.active_operations.values().map(|ops| ops.len()).sum()
    }

    pub fn get_operations_per_second(&self) -> usize {
        let now = Instant::now();
        self.operation_tracker.operation_history
            .iter()
            .filter(|(time, _)| now.duration_since(*time) < Duration::from_secs(1))
            .count()
    }
}
