//! Ed25519 signing: the [`Signer`] trait, an in-memory [`Keypair`] (feature `sign`) and [`verify`].
//! Messages are signed as raw bytes with no hash or prefix, the same as a wallet's `signMessage`.

use crate::utils::keys::{Pubkey, Signature};

/// Signs permit bytes. Implemented by [`Keypair`]; a hardware or remote signer can also implement it.
pub trait Signer {
    /// The signing public key, sent as the request's `auth.signer`.
    fn pubkey(&self) -> Pubkey;

    /// Ed25519 signature over the raw `message` bytes.
    fn sign_message(&self, message: &[u8]) -> Result<Signature, SignError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SignError {
    #[error("the signer refused: {0}")]
    Refused(String),
    #[error("this build has no Ed25519 (feature `sign`)")]
    NoCurve,
    #[error("the signature does not verify under {0}")]
    Invalid(Pubkey),
}

impl<S: Signer + ?Sized> Signer for &S {
    fn pubkey(&self) -> Pubkey {
        (**self).pubkey()
    }

    fn sign_message(&self, message: &[u8]) -> Result<Signature, SignError> {
        (**self).sign_message(message)
    }
}

/// Checks `signature` over `message` against `signer` with strict Ed25519 verification, as the
/// Solana runtime does.
#[cfg(feature = "sign")]
#[must_use]
pub fn verify(signer: &Pubkey, message: &[u8], signature: &Signature) -> bool {
    let Ok(key) = ed25519_dalek::VerifyingKey::from_bytes(signer.as_bytes()) else {
        return false;
    };
    key.verify_strict(message, &ed25519_dalek::Signature::from_bytes(signature.as_bytes())).is_ok()
}

#[cfg(feature = "sign")]
pub use keypair::Keypair;

#[cfg(feature = "sign")]
mod keypair {
    use std::fmt;

    use ed25519_dalek::{Signer as _, SigningKey};
    use zeroize::Zeroizing;

    use crate::utils::base58;
    use crate::utils::keys::{BadKey, Pubkey, Signature, parse_secret};
    use crate::utils::sign::{SignError, Signer};

    /// An in-memory Ed25519 keypair. The seed is zeroed on drop and is not shown by `Debug`.
    #[derive(Clone)]
    pub struct Keypair {
        key: SigningKey,
    }

    impl Keypair {
        #[must_use]
        pub fn from_seed(seed: &[u8; 32]) -> Keypair {
            Keypair { key: SigningKey::from_bytes(seed) }
        }

        /// Parses any format [`parse_secret`] accepts. A 64-byte secret key whose public key does
        /// not match its seed is an error.
        pub fn parse(text: &str) -> Result<Keypair, BadKey> {
            let (seed, public) = parse_secret(text)?;
            let keypair = Keypair::from_seed(&seed);
            match public {
                Some(public) if public != keypair.pubkey() => Err(BadKey::SecretMismatch),
                _ => Ok(keypair),
            }
        }

        /// Generates a key from OS randomness (`crypto.getRandomValues` on wasm).
        pub fn generate() -> Result<Keypair, SignError> {
            let mut seed = Zeroizing::new([0u8; 32]);
            getrandom02::getrandom(seed.as_mut()).map_err(|error| SignError::Refused(error.to_string()))?;
            Ok(Keypair::from_seed(&seed))
        }

        #[must_use]
        pub fn pubkey(&self) -> Pubkey {
            Pubkey::new(self.key.verifying_key().to_bytes())
        }

        #[must_use]
        pub fn sign(&self, message: &[u8]) -> Signature {
            Signature::new(self.key.sign(message).to_bytes())
        }

        /// The 32-byte seed in base58, the format the Python quickstart prints. This is secret.
        #[must_use]
        pub fn seed_base58(&self) -> Zeroizing<String> {
            Zeroizing::new(base58::encode(self.key.as_bytes()))
        }

        /// Seed and public key, 64 bytes in base58: the Phantom and Solflare export format.
        #[must_use]
        pub fn secret_base58(&self) -> Zeroizing<String> {
            Zeroizing::new(base58::encode(&self.key.to_keypair_bytes()))
        }
    }

    impl Signer for Keypair {
        fn pubkey(&self) -> Pubkey {
            Keypair::pubkey(self)
        }

        fn sign_message(&self, message: &[u8]) -> Result<Signature, SignError> {
            Ok(self.sign(message))
        }
    }

    /// Prints only the public key.
    impl fmt::Debug for Keypair {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Keypair").field("pubkey", &self.pubkey()).finish_non_exhaustive()
        }
    }
}

#[cfg(test)]
#[cfg(feature = "sign")]
#[path = "tests/sign.rs"]
mod tests;
