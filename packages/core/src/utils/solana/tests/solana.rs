use super::*;

#[test]
fn a_pascal_deposit_address_s_usdc_account_on_mainnet_derives() {
    // Read from mainnet: the one USDC account a registered deposit address holds.
    let deposit_address = Pubkey::parse("8c5x1Prxqzqq1RBAvjmQeJAEkS6T8mgndU5e5uMfCDnB").unwrap();
    assert_eq!(usdc_account(&deposit_address).to_base58(), "PZhER1fgPXuECRqVJT9e6J5pFtp77CjuD2m62rjVfJM");
}

#[test]
fn every_program_id_reads() {
    for id in [
        TOKEN_PROGRAM,
        TOKEN_2022_PROGRAM,
        ASSOCIATED_TOKEN_PROGRAM,
        SYSTEM_PROGRAM,
        COMPUTE_BUDGET_PROGRAM,
        USDC_MINT,
        PASCAL_PROGRAM,
    ] {
        assert!(Pubkey::parse(id).is_ok(), "{id}");
    }
}

#[test]
fn a_derived_address_is_off_the_curve_and_seeds_are_bounded() {
    let program = Pubkey::from_literal(PASCAL_PROGRAM);
    let (address, _bump) = find_program_address(&[b"seed"], &program).unwrap();
    assert!(!address.is_on_curve());
    assert_eq!(find_program_address(&[&[0u8; 33]], &program), Err(BadSeeds::TooLong));
}
