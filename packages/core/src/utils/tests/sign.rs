use super::*;
use crate::utils::keys::BadKey;

#[test]
fn a_keypair_signs_and_verifies_and_hides_its_seed() {
    let key = Keypair::from_seed(&[8; 32]);
    let signature = key.sign(b"message");
    assert!(verify(&key.pubkey(), b"message", &signature));
    assert!(!verify(&key.pubkey(), b"messagf", &signature));
    assert!(!format!("{key:?}").contains("0808"));
    let again = Keypair::parse(&key.seed_base58()).unwrap();
    assert_eq!(again.pubkey(), key.pubkey());
    let full = Keypair::parse(&key.secret_base58()).unwrap();
    assert_eq!(full.pubkey(), key.pubkey());
}

#[test]
fn a_64_byte_secret_whose_halves_disagree_is_refused() {
    let mut bytes = [8u8; 64];
    bytes[63] ^= 1;
    let text = crate::utils::base58::encode(&bytes);
    assert_eq!(Keypair::parse(&text).unwrap_err(), BadKey::SecretMismatch);
}

#[test]
fn generated_keys_differ() {
    assert_ne!(Keypair::generate().unwrap().pubkey(), Keypair::generate().unwrap().pubkey());
}
