use super::standards::{ContractResult, ContractError};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::msg;

/// Role-based access control event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleGrantedEvent {
    pub role: [u8; 32],
    pub account: [u8; 32],
    pub sender: [u8; 32],
}

/// Role revocation event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleRevokedEvent {
    pub role: [u8; 32],
    pub account: [u8; 32],
    pub sender: [u8; 32],
}

/// Access control implementation
#[derive(Debug)]
pub struct AccessControl {
    /// Role assignments
    roles: HashMap<[u8; 32], HashMap<[u8; 32], bool>>,
    /// Role admins
    role_admins: HashMap<[u8; 32], [u8; 32]>,
}

impl AccessControl {
    /// Create new access control instance
    pub fn new() -> Self {
        let mut access_control = AccessControl {
            roles: HashMap::new(),
            role_admins: HashMap::new(),
        };

        // Set up default admin role
        access_control.role_admins.insert(DEFAULT_ADMIN_ROLE, DEFAULT_ADMIN_ROLE);
        
        access_control
    }

    /// Check if an account has a role
    pub fn check_role(&self, role: [u8; 32], account: &[u8; 32]) -> ContractResult<()> {
        let has_role = self.roles
            .get(&role)
            .and_then(|accounts| accounts.get(account))
            .copied()
            .unwrap_or(false);

        if !has_role {
            return Err(ContractError::AccessDenied(format!(
                "Account {:?} does not have required role {:?}",
                account, role
            )));
        }

        Ok(())
    }

    /// Grant a role to an account
    pub fn grant_role(&mut self, role: [u8; 32], account: [u8; 32]) -> ContractResult<bool> {
        // Get sender
        let sender = msg::sender().map_err(|e| ContractError::ExecutionError(e))?;

        // Special case: Allow granting DEFAULT_ADMIN_ROLE if no accounts have it yet
        let is_first_admin = role == DEFAULT_ADMIN_ROLE && 
            !self.roles.contains_key(&DEFAULT_ADMIN_ROLE);

        if !is_first_admin {
            // Check if sender has admin role
            let admin_role = self.get_role_admin(role);
            if let Err(e) = self.check_role(admin_role, &sender) {
                return Err(ContractError::AccessDenied(format!(
                    "Sender does not have admin role for role {:?}: {}",
                    role, e
                )));
            }
        }

        // Check if account already has role
        if self.has_role(role, &account) {
            return Ok(false);
        }

        // Grant the role
        self.roles
            .entry(role)
            .or_insert_with(HashMap::new)
            .insert(account, true);

        Ok(true)
    }

    /// Check if an account has a role without generating error
    pub fn has_role(&self, role: [u8; 32], account: &[u8; 32]) -> bool {
        self.roles
            .get(&role)
            .and_then(|accounts| accounts.get(account))
            .copied()
            .unwrap_or(false)
    }

    /// Revoke a role from an account
    pub fn revoke_role(&mut self, role: [u8; 32], account: [u8; 32]) -> ContractResult<bool> {
        // Get sender
        let sender = msg::sender().map_err(|e| ContractError::ExecutionError(e))?;

        // Check if sender has admin role
        let admin_role = self.get_role_admin(role);
        if let Err(e) = self.check_role(admin_role, &sender) {
            return Err(ContractError::AccessDenied(format!(
                "Sender does not have admin role for role {:?}: {}",
                role, e
            )));
        }

        // Check if account has role
        if !self.has_role(role, &account) {
            return Ok(false);
        }

        // Revoke the role
        if let Some(accounts) = self.roles.get_mut(&role) {
            accounts.remove(&account);
        }

        Ok(true)
    }

    /// Get the admin role for a role
    pub fn get_role_admin(&self, role: [u8; 32]) -> [u8; 32] {
        self.role_admins.get(&role).copied().unwrap_or(DEFAULT_ADMIN_ROLE)
    }

    /// Set the admin role for a role
    pub fn set_role_admin(&mut self, role: [u8; 32], admin_role: [u8; 32]) -> ContractResult<()> {
        // Get sender
        let sender = msg::sender().map_err(|e| ContractError::ExecutionError(e))?;

        // Check if sender has current admin role
        let current_admin = self.get_role_admin(role);
        if let Err(e) = self.check_role(current_admin, &sender) {
            return Err(ContractError::AccessDenied(format!(
                "Sender does not have current admin role for role {:?}: {}",
                role, e
            )));
        }

        // Cannot change admin of DEFAULT_ADMIN_ROLE
        if role == DEFAULT_ADMIN_ROLE {
            return Err(ContractError::InvalidOperation(
                "Cannot change admin role of DEFAULT_ADMIN_ROLE".into()
            ));
        }

        self.role_admins.insert(role, admin_role);
        Ok(())
    }
}

/// Reentrancy guard to prevent recursive calls
pub struct ReentrancyGuard {
    /// Lock status
    entered: Arc<Mutex<bool>>,
}

impl ReentrancyGuard {
    /// Create new reentrancy guard
    pub fn new() -> Self {
        ReentrancyGuard {
            entered: Arc::new(Mutex::new(false)),
        }
    }

    /// Enter the guarded section
    pub fn enter(&self) -> ContractResult<()> {
        let mut guard = self.entered.lock().map_err(|e| ContractError::LockError(format!(
            "Failed to acquire reentrancy lock: {}", e
        )))?;

        if *guard {
            return Err(ContractError::ReentrancyError(
                "Reentrant call detected".into()
            ));
        }

        *guard = true;
        Ok(())
    }

    /// Exit the guarded section
    pub fn exit(&self) {
        if let Ok(mut guard) = self.entered.lock() {
            *guard = false;
        } else {
            // Log error but don't panic as this is cleanup code
            eprintln!("Warning: Failed to release reentrancy lock");
        }
    }
}

/// Default admin role
pub const DEFAULT_ADMIN_ROLE: [u8; 32] = [0u8; 32];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_control() {
        let mut access = AccessControl::new();
        let account = [1u8; 32];
        let role = [2u8; 32];

        // Initially account should not have role
        assert!(matches!(
            access.check_role(role, &account),
            Err(ContractError::AccessDenied(_))
        ));

        // Set up test sender
        msg::test_utils::set_sender([1u8; 32]).unwrap();

        // First grant admin role
        assert!(access.grant_role(DEFAULT_ADMIN_ROLE, account).unwrap());

        // Now we can grant other roles
        assert!(access.grant_role(role, account).unwrap());

        // Clean up
        msg::test_utils::clear_sender().unwrap();
    }

    #[test]
    fn test_reentrancy_guard() {
        let guard = ReentrancyGuard::new();

        // First enter should succeed
        assert!(guard.enter().is_ok());

        // Second enter should fail with ReentrancyError
        assert!(matches!(
            guard.enter(),
            Err(ContractError::ReentrancyError(_))
        ));

        // After exit, enter should succeed again
        guard.exit();
        assert!(guard.enter().is_ok());
    }

    #[test]
    fn test_role_admin() {
        let mut access = AccessControl::new();
        let account = [1u8; 32];
        let role = [1u8; 32];
        let admin_role = [2u8; 32];

        // Initially should have default admin
        assert_eq!(access.get_role_admin(role), DEFAULT_ADMIN_ROLE);

        // Set up test sender
        msg::test_utils::set_sender(account).unwrap();

        // First grant admin role
        assert!(access.grant_role(DEFAULT_ADMIN_ROLE, account).unwrap());

        // Now we can set new admin role
        assert!(access.set_role_admin(role, admin_role).is_ok());

        // Cannot change DEFAULT_ADMIN_ROLE's admin
        assert!(matches!(
            access.set_role_admin(DEFAULT_ADMIN_ROLE, admin_role),
            Err(ContractError::InvalidOperation(_))
        ));

        // Clean up
        msg::test_utils::clear_sender().unwrap();
    }
}
