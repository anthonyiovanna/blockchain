use thiserror::Error;

/// Contract operation result type
pub type ContractResult<T> = Result<T, ContractError>;

/// Contract error types
#[derive(Debug, Error)]
pub enum ContractError {
    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("Contract not found: {0}")]
    NotFound(String),

    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("Compilation error: {0}")]
    CompilationError(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Lock error: {0}")]
    LockError(String),

    #[error("Reentrancy error: {0}")]
    ReentrancyError(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    // Version-specific errors
    #[error("Version conflict: {0}")]
    VersionConflict(String),

    #[error("Version not found: {0}")]
    VersionNotFound(String),

    #[error("Version incompatible: {0}")]
    VersionIncompatible(String),

    #[error("Version upgrade failed: {0}")]
    VersionUpgradeFailed(String),

    // State-specific errors
    #[error("State error: {0}")]
    StateError(String),

    #[error("State validation error: {0}")]
    StateValidationError(String),

    #[error("State corrupted: {0}")]
    StateCorrupted(String),

    #[error("State rollback failed: {0}")]
    StateRollbackFailed(String),

    // Upgrade-specific errors
    #[error("Upgrade authorization error: {0}")]
    UpgradeAuthorizationError(String),

    #[error("Upgrade validation error: {0}")]
    UpgradeValidationError(String),

    #[error("Upgrade rollback error: {0}")]
    UpgradeRollbackError(String),

    #[error("Upgrade limit exceeded: {0}")]
    UpgradeLimitExceeded(String),

    // Bytecode verification errors
    #[error("Bytecode verification error: {0}")]
    BytecodeVerificationError(String),

    #[error("Bytecode integrity error: {0}")]
    BytecodeIntegrityError(String),

    // Concurrency-specific errors
    #[error("Concurrency limit exceeded: {0}")]
    ConcurrencyLimitExceeded(String),

    #[error("Operation timeout: {0}")]
    OperationTimeout(String),

    #[error("Operation conflict: {0}")]
    OperationConflict(String),
}

impl ContractError {
    /// Returns true if the error is related to version conflicts
    pub fn is_version_error(&self) -> bool {
        matches!(
            self,
            ContractError::VersionConflict(_) |
            ContractError::VersionNotFound(_) |
            ContractError::VersionIncompatible(_) |
            ContractError::VersionUpgradeFailed(_)
        )
    }

    /// Returns true if the error is related to state validation
    pub fn is_state_error(&self) -> bool {
        matches!(
            self,
            ContractError::StateError(_) |
            ContractError::StateValidationError(_) |
            ContractError::StateCorrupted(_) |
            ContractError::StateRollbackFailed(_)
        )
    }

    /// Returns true if the error is related to upgrades
    pub fn is_upgrade_error(&self) -> bool {
        matches!(
            self,
            ContractError::UpgradeAuthorizationError(_) |
            ContractError::UpgradeValidationError(_) |
            ContractError::UpgradeRollbackError(_) |
            ContractError::UpgradeLimitExceeded(_)
        )
    }

    /// Returns true if the error is related to bytecode verification
    pub fn is_bytecode_error(&self) -> bool {
        matches!(
            self,
            ContractError::BytecodeVerificationError(_) |
            ContractError::BytecodeIntegrityError(_)
        )
    }

    /// Returns true if the error is related to concurrency
    pub fn is_concurrency_error(&self) -> bool {
        matches!(
            self,
            ContractError::ConcurrencyLimitExceeded(_) |
            ContractError::OperationTimeout(_) |
            ContractError::OperationConflict(_)
        )
    }

    /// Returns true if the error is recoverable
    pub fn is_recoverable(&self) -> bool {
        !matches!(
            self,
            ContractError::StateCorrupted(_) |
            ContractError::BytecodeIntegrityError(_)
        )
    }
}
