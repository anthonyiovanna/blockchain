# Change Log: crypto.rs

## [2024-01-09]
- Added derive macros for Hash struct:
  * Eq, Hash, PartialEq (for HashMap/HashSet support)
  * Serialize, Deserialize (for serde support)
  * Default (for struct initialization)
- Enhanced Signature struct:
  * Added Clone, Debug traits
  * Added Serialize, Deserialize with serde_bytes
  * Changed internal storage to Vec<u8>
  * Added proper conversion methods for ed25519_dalek
- Fixed KeyPair::from_seed error handling
- Added new tests for serialization
