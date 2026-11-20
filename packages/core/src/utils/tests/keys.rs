use super::*;

const OWNER: &str = "GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB";

#[test]
fn a_pubkey_reads_and_writes_base58_and_keeps_its_case() {
    let key = Pubkey::parse(OWNER).unwrap();
    assert_eq!(key.to_string(), OWNER);
    assert!(
        Pubkey::parse(&OWNER.to_lowercase()).is_ok_and(|lower| lower != key)
            || Pubkey::parse(&OWNER.to_lowercase()).is_err()
    );
    assert_eq!(serde_json::to_string(&key).unwrap(), format!("\"{OWNER}\""));
    assert_eq!(serde_json::from_str::<Pubkey>(&format!("\"{OWNER}\"")).unwrap(), key);
    assert!(Pubkey::parse("2KW2XRd9").is_err());
    assert_eq!(key.short(), "GmaD..AEGB");
}

#[test]
fn every_secret_spelling_reads_to_the_same_seed() {
    let seed = [1u8; 32];
    let json32 = format!("[{}]", vec!["1"; 32].join(","));
    let hex = "01".repeat(32);
    let base58 = crate::utils::base58::encode(&seed);
    for text in [json32.as_str(), hex.as_str(), base58.as_str()] {
        let (read, public) = parse_secret(text).unwrap();
        assert_eq!(*read, seed);
        assert!(public.is_none());
    }
}

#[test]
fn a_solana_cli_keypair_file_reads_with_its_public_half() {
    // The onboarding docs' example: seed of 0x01 bytes, then its public key.
    let text = "[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,138,136,227,221,116,9,241,149,253,82,219,45,60,186,93,114,202,103,9,191,29,148,18,27,243,116,136,1,180,15,111,92]";
    let (seed, public) = parse_secret(text).unwrap();
    assert_eq!(*seed, [1u8; 32]);
    assert!(public.is_some());
    #[cfg(feature = "sign")]
    assert_eq!(crate::utils::sign::Keypair::parse(text).unwrap().pubkey(), public.unwrap());
}

#[test]
fn bad_secrets_are_refused_with_the_reason() {
    assert_eq!(parse_secret("  "), Err(BadKey::EmptySecret));
    assert_eq!(parse_secret("[1,2,3]"), Err(BadKey::SecretLength(3)));
    assert!(matches!(parse_secret("[1,999]"), Err(BadKey::Secret(_))));
}

#[test]
fn a_program_derived_address_is_off_the_curve() {
    assert!(Pubkey::parse(OWNER).unwrap().is_on_curve());
    // Pascal's deposit address for the docs' owner is a PDA.
    assert!(!Pubkey::parse("DtzBRq9hAREJZKj1fuHsgrWrtaeXmbYQdXsmMrd54arG").unwrap().is_on_curve());
}
