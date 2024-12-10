use std::sync::RwLock;
use once_cell::sync::Lazy;

// Global sender for testing
static TEST_SENDER: Lazy<RwLock<Option<[u8; 32]>>> = Lazy::new(|| RwLock::new(None));

/// Get the sender address
pub fn sender() -> Result<[u8; 32], String> {
    if cfg!(test) {
        // In test mode, return the test sender or a default
        Ok(TEST_SENDER
            .read()
            .map_err(|e| format!("Failed to read test sender: {}", e))?
            .unwrap_or([1u8; 32]))
    } else {
        // TODO: Implement proper sender tracking for production
        Ok([0u8; 32])
    }
}

// Testing utilities available for both unit and integration tests
pub mod test_utils {
    use super::*;

    /// Set the test sender
    pub fn set_sender(addr: [u8; 32]) -> Result<(), String> {
        TEST_SENDER
            .write()
            .map_err(|e| format!("Failed to write test sender: {}", e))
            .map(|mut guard| *guard = Some(addr))
    }

    /// Clear the test sender
    pub fn clear_sender() -> Result<(), String> {
        TEST_SENDER
            .write()
            .map_err(|e| format!("Failed to write test sender: {}", e))
            .map(|mut guard| *guard = None)
    }
}

// Re-export test utilities under testing namespace for backward compatibility in unit tests
#[cfg(test)]
pub use test_utils as testing;
