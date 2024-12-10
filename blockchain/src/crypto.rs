use ed25519_dalek::{Signature as EdSignature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use std::fmt;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, Default)]
pub struct Hash([u8; 32]);

impl Hash {
    pub fn new(data: &[u8]) -> Self {
        let hash = blake3::hash(data);
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(hash.as_bytes());
        Hash(bytes)
    }

    pub fn to_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signature(#[serde(with = "serde_bytes")] Vec<u8>);

impl Signature {
    pub fn to_bytes(&self) -> &[u8] {
        &self.0
    }

    fn from_ed_signature(sig: EdSignature) -> Self {
        Signature(sig.to_bytes().to_vec())
    }

    pub fn to_ed_signature(&self) -> Result<EdSignature, ed25519_dalek::SignatureError> {
        EdSignature::from_slice(&self.0)
    }
}

pub struct KeyPair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl KeyPair {
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        KeyPair {
            signing_key,
            verifying_key,
        }
    }

    pub fn from_seed(seed: &[u8]) -> Result<Self, ed25519_dalek::SignatureError> {
        if seed.len() != 32 {
            return Err(ed25519_dalek::SignatureError::from_source("Invalid seed length"));
        }
        let mut seed_bytes = [0u8; 32];
        seed_bytes.copy_from_slice(seed);
        let signing_key = SigningKey::from_bytes(&seed_bytes);
        let verifying_key = signing_key.verifying_key();
        Ok(KeyPair {
            signing_key,
            verifying_key,
        })
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        let ed_signature = self.signing_key.sign(message);
        Signature::from_ed_signature(ed_signature)
    }

    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        if let Ok(ed_signature) = signature.to_ed_signature() {
            self.verifying_key
                .verify(message, &ed_signature)
                .is_ok()
        } else {
            false
        }
    }

    pub fn public_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_creation() {
        let data = b"test data";
        let hash = Hash::new(data);
        assert_eq!(hash.0.len(), 32);
    }

    #[test]
    fn test_hash_to_hex() {
        let data = b"test data";
        let hash = Hash::new(data);
        let hex = hash.to_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_keypair_generation() {
        let keypair = KeyPair::generate();
        let message = b"test message";
        let signature = keypair.sign(message);
        assert!(keypair.verify(message, &signature));
    }

    #[test]
    fn test_keypair_from_seed() {
        let seed = [1u8; 32];
        let keypair = KeyPair::from_seed(&seed).unwrap();
        let message = b"test message";
        let signature = keypair.sign(message);
        assert!(keypair.verify(message, &signature));
    }

    #[test]
    fn test_signature_verification() {
        let keypair = KeyPair::generate();
        let message = b"test message";
        let signature = keypair.sign(message);
        assert!(keypair.verify(message, &signature));
        assert!(!keypair.verify(b"wrong message", &signature));
    }

    #[test]
    fn test_hash_serialization() {
        let hash = Hash::new(b"test data");
        let serialized = serde_json::to_string(&hash).unwrap();
        let deserialized: Hash = serde_json::from_str(&serialized).unwrap();
        assert_eq!(hash.0, deserialized.0);
    }

    #[test]
    fn test_signature_serialization() {
        let keypair = KeyPair::generate();
        let signature = keypair.sign(b"test message");
        let serialized = serde_json::to_string(&signature).unwrap();
        let deserialized: Signature = serde_json::from_str(&serialized).unwrap();
        assert_eq!(signature.0, deserialized.0);
    }
}
