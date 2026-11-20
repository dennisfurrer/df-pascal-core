//! Signing vectors from Pascal's docs and the Python quickstart: expected permit bytes and
//! signatures.

use super::*;
use crate::utils::keys::Signature;
use crate::utils::sign::Keypair;

const OWNER: &str = "GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB";
const TRADING: &str = "2KW2XRd9kwqet15Aha2oK3tYvd3nWbTFH1MBiRAv1BE1";
const BUILDER: &str = "J2xccRtuG43drESLYznHhLhQkLTdfepcKYbiQ9BsJVaf";

fn owner_key() -> Keypair {
    Keypair::from_seed(&[7; 32])
}

fn trading_key() -> Keypair {
    Keypair::from_seed(&[8; 32])
}

fn auth(signer: &str) -> Auth {
    Auth {
        deployment_id: 3,
        owner: Pubkey::from_literal(OWNER),
        signer: Pubkey::from_literal(signer),
        client_ts_ms: 1_731_536_000_000,
        recv_window_ms: 5_000,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn signs(key: &Keypair, message: &[u8], signature: &str) {
    assert_eq!(key.sign(message).to_base58(), signature);
    assert!(crate::utils::sign::verify(&key.pubkey(), message, &Signature::parse(signature).unwrap()));
}

fn docs_order() -> PlaceOrder {
    PlaceOrder {
        client_order_id: 42,
        replace_client_order_id: Some(43),
        symbol: "SIM_EVENT_1.MARKET_1".to_owned(),
        side: Side::Bid,
        price: dec("0.550000"),
        size: 10,
        tif: TimeInForce::Gtt,
        order_type: OrderType::Limit,
        expires_ts_ms: Some(1_731_539_600_000),
        post_only: true,
        reduce_only: true,
        allow_missing_replace: false,
        replace_with_open_size: false,
        builder: None,
        metadata: 0,
    }
}

#[test]
fn the_keys_derive_the_docs_public_keys() {
    assert_eq!(owner_key().pubkey().to_base58(), OWNER);
    assert_eq!(trading_key().pubkey().to_base58(), TRADING);
    assert_eq!(Keypair::from_seed(&[9; 32]).pubkey().to_base58(), BUILDER);
}

#[test]
fn the_offchain_fields_digest_is_the_hash_of_its_tag() {
    assert_eq!(offchain_fields_digest(), OFFCHAIN_FIELDS_DIGEST);
}

#[test]
fn place_order_matches_the_docs_vector() {
    let order = PlaceOrder { allow_missing_replace: true, ..docs_order() };
    let message = place_order(&auth(TRADING), &order).unwrap();
    assert_eq!(message.len(), PLACE_ORDER_LEN);
    assert_eq!(
        hex(&message),
        "01010300881300000094962793010000ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c1398f62c6d1a457c51ba6a4b5f3dbd2f69fca93216218dc8997e416bd17d93ca0a0000000000000070640800000000002a000000000000002b000000000000008082cd2793010000000000000000000000000000000000000000000000000000000000000000000053494d5f4556454e545f312e4d41524b45545f3100000000000000000000000000000000dcaf53738f1d5f06164660498a75888e7f0c78ed5f1bc17be1dd1cb3ccf53ce3000000010700000000000000"
    );
    signs(
        &trading_key(),
        &message,
        "46gXGMwTxEckctHD3tjVyedhGG95VK2F97F3idkSYFR3WNEX4rG9g6g8KDTQPoxzLieudk8WHDXw7xNV3R2WwKCd",
    );
}

#[test]
fn place_order_matches_the_quickstart_vectors_with_and_without_a_builder() {
    let plain = PlaceOrder { metadata: 2, ..docs_order() };
    let message = place_order(&auth(TRADING), &plain).unwrap();
    assert_eq!(
        hex(&message),
        "01010300881300000094962793010000".to_owned()
            + "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c"
            + "1398f62c6d1a457c51ba6a4b5f3dbd2f69fca93216218dc8997e416bd17d93ca"
            + "0a0000000000000070640800000000002a000000000000002b00000000000000"
            + "8082cd279301000000000000000000000000000000000000000000000000000000"
            + "0000000000000053494d5f4556454e545f312e4d41524b45545f310000000000"
            + "0000000000000000000000dcaf53738f1d5f06164660498a75888e7f0c78ed"
            + "5f1bc17be1dd1cb3ccf53ce3020000010300000000000000"
    );
    signs(
        &trading_key(),
        &message,
        "3uG7HnnR2j3R26LoW3VNSJzKUfs16M9kXc58JbhkACwm2KmBXQuozLCQvREuAoxuHQjzADdBTV7NkgNrqRANgaZN",
    );

    let with_builder = PlaceOrder {
        builder: Some(Builder { pubkey: Pubkey::from_literal(BUILDER), fee_rate: dec("0.010000") }),
        ..plain
    };
    let message = place_order(&auth(TRADING), &with_builder).unwrap();
    assert_eq!(&message[120..152], Pubkey::from_literal(BUILDER).as_bytes());
    assert_eq!(&message[184..188], &10_000i32.to_le_bytes());
    signs(
        &trading_key(),
        &message,
        "2jsmMEct5WwmYHd1V8syLLTmxrLDVRChBScHnQpM48aNuqKvyqWt55pUWocTZiiogJm3SEQkLKzJDEvuUDh1eiEH",
    );
}

#[test]
fn place_order_refuses_what_pascal_refuses() {
    let owner = Pubkey::from_literal(OWNER);
    let base = docs_order();
    let refused = [
        PlaceOrder { price: dec("1.000001"), ..base.clone() },
        PlaceOrder { price: dec("0.5500001"), ..base.clone() },
        PlaceOrder { size: 0, ..base.clone() },
        PlaceOrder { client_order_id: u64::MAX, ..base.clone() },
        PlaceOrder { expires_ts_ms: None, ..base.clone() },
        PlaceOrder { tif: TimeInForce::Gtc, ..base.clone() },
        PlaceOrder { order_type: OrderType::Market, ..base.clone() },
        PlaceOrder { replace_client_order_id: None, allow_missing_replace: true, ..base.clone() },
        PlaceOrder { allow_missing_replace: true, replace_with_open_size: true, ..base.clone() },
        PlaceOrder { symbol: "X".repeat(33), ..base.clone() },
        PlaceOrder { builder: Some(Builder { pubkey: owner, fee_rate: dec("0.01") }), ..base.clone() },
        PlaceOrder {
            builder: Some(Builder { pubkey: Pubkey::from_literal(BUILDER), fee_rate: dec("0.100001") }),
            ..base.clone()
        },
        PlaceOrder {
            builder: Some(Builder { pubkey: Pubkey::from_literal(BUILDER), fee_rate: dec("-0.01") }),
            ..base.clone()
        },
        PlaceOrder { builder: Some(Builder { pubkey: Pubkey::ZERO, fee_rate: dec("0.01") }), ..base.clone() },
    ];
    for order in refused {
        assert!(order.check(&owner).is_err(), "{order:?}");
    }
    assert!(base.check(&owner).is_ok());
    let open_size = PlaceOrder { replace_with_open_size: true, ..base };
    assert_eq!(place_order(&auth(TRADING), &open_size).unwrap()[224], 0b1011);
}

#[test]
fn cancel_matches_the_docs_and_quickstart_vectors() {
    let message = cancel_order(&auth(TRADING), CancelBy::ClientOrderId(42), 0).unwrap();
    assert_eq!(
        hex(&message),
        "01020300881300000094962793010000ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c1398f62c6d1a457c51ba6a4b5f3dbd2f69fca93216218dc8997e416bd17d93ca00000000000000002a00000000000000"
    );
    signs(
        &trading_key(),
        &message,
        "591YziUjZzro24mamxgRrSDdYD71NpsDBmU4UGSyrBYbFiWzYaLjR9he1P5DP8djMKMJzHTsZzwRAagvLqgtiXnc",
    );
    let message = cancel_order(&auth(TRADING), CancelBy::ClientOrderId(42), 2).unwrap();
    signs(
        &trading_key(),
        &message,
        "CZMtBC9qYUExsoWAvjoXCwavWqpzKJdgTwGXypJYRFVs9cgzdRGhgJBSinCS814mJrfyattRVNdnnfcPE36ptMi",
    );
    let message = cancel_order(&auth(TRADING), CancelBy::ClientOrderId(43), 0).unwrap();
    signs(
        &trading_key(),
        &message,
        "3wFZKJWKA92W6iPszoKQccQmrVEfd6hU68HY6mqWYKEH8yFKQEFRJs1tKmrGDy1PMd3wWwcdcYSVoTgrW5KSnP3f",
    );
    assert_eq!(cancel_order(&auth(TRADING), CancelBy::OrderId(42), 0).unwrap()[80], 1);
}

#[test]
fn mass_cancel_matches_the_docs_vector() {
    let message = mass_cancel(&auth(TRADING), "SIM_EVENT_1.MARKET_1").unwrap();
    assert_eq!(
        hex(&message),
        "010a0300881300000094962793010000ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c1398f62c6d1a457c51ba6a4b5f3dbd2f69fca93216218dc8997e416bd17d93ca000000000000000053494d5f4556454e545f312e4d41524b45545f31000000000000000000000000"
    );
    signs(
        &trading_key(),
        &message,
        "oLmkQxx9xLPK4QJnFhZUFEker5k5pbLregsYPgRrhrd9sEBYDuCtitUtFSVfhiJpTFHWAdmGk6zKDLpPtA3iegS",
    );
}

#[test]
fn collateral_return_matches_the_docs_vector() {
    let message = collateral_return(&auth(TRADING), "SIM_EVENT_1").unwrap();
    assert_eq!(
        hex(&message),
        "010b0300881300000094962793010000ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c1398f62c6d1a457c51ba6a4b5f3dbd2f69fca93216218dc8997e416bd17d93ca53494d5f4556454e545f31000000000000000000000000000000000000000000"
    );
    signs(
        &trading_key(),
        &message,
        "46qFmNPjoGhZujGPWRe41aLmeCvixwMy36BCAySmgxiy9Y4pvCHfhSMoGTRzg3czNmsEEVAveH7pbriDTEmgXYDM",
    );
}

fn text(message: &[u8]) -> &str {
    std::str::from_utf8(message).unwrap()
}

#[test]
fn wallet_permits_match_the_docs_vectors() {
    let wallet = auth(OWNER);
    let message =
        create_trading_key(&wallet, "api-doc-key", &Pubkey::from_literal(TRADING), 1_732_140_800_000).unwrap();
    assert_eq!(
        text(&message),
        r#"["create_trading_key","api-doc-key","2KW2XRd9kwqet15Aha2oK3tYvd3nWbTFH1MBiRAv1BE1",1732140800000,1,3,5000,1731536000000,"GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB","GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB"]"#
    );
    signs(
        &owner_key(),
        &message,
        "kyc2KdqPTeEWqoK3pXR2rgPUiK4zXg9QdQffPUWk8eALpFy7uaHBEsASt7RQyVzDeubHVsPqQAHXrt8m8KZ78Ep",
    );

    let message = revoke_trading_key(&wallet, &Pubkey::from_literal(TRADING)).unwrap();
    assert_eq!(
        text(&message),
        r#"["revoke_trading_key","2KW2XRd9kwqet15Aha2oK3tYvd3nWbTFH1MBiRAv1BE1",1,3,5000,1731536000000,"GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB","GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB"]"#
    );
    signs(
        &owner_key(),
        &message,
        "3muKct6upyRNAr1CPdYcxQGCojpuB4gaEnWvbNwEBJ6rrKaThRZrKY56rpYbFEsGUZqJ4iCTJqYrPuxfGYVzXQX3",
    );

    let message = withdrawal(
        &wallet,
        25_000_000,
        &Pubkey::from_literal("29d2S7vB453rNYFdR5Ycwt7y9haRT5fwVwL9zTmBhfV2"),
        &Pubkey::from_literal("3JF3sEqM796hk5WFqA6EtmEwJQ9quALszsfJyvXNQKy3"),
    )
    .unwrap();
    assert_eq!(
        text(&message),
        r#"["withdrawal",25000000,"29d2S7vB453rNYFdR5Ycwt7y9haRT5fwVwL9zTmBhfV2","3JF3sEqM796hk5WFqA6EtmEwJQ9quALszsfJyvXNQKy3",1,3,5000,1731536000000,"GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB","GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB"]"#
    );
    signs(
        &owner_key(),
        &message,
        "61YYUBk6BTXkfrksMbqDLUNxwo6jhMPqdP7fNnWjNF2FYvMDujTsaUxfCiRxXX72jxy4oLtt6AtLodCvjDi4XpDn",
    );

    let message = register_deposit_address(&wallet, Some("k7wq2m4x")).unwrap();
    assert_eq!(
        text(&message),
        r#"["register_deposit_address","GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB",1,3,5000,1731536000000,"GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB","GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB","referral_code","k7wq2m4x"]"#
    );
    signs(
        &owner_key(),
        &message,
        "2GVZuf1birCpfr6DKbu1ALSxRTpVnzFAa6oYqnEsnDUB2TFqeYTsKEZrCxNzATyCZbEaLW7koZJPRh9euhYiLtm4",
    );

    let message = register_deposit_address(&wallet, None).unwrap();
    signs(
        &owner_key(),
        &message,
        "5xWT1auD2tHfqw3arxJqQC4XdZVXYMcT382wCLAHPdequru7LZJ9Dsb8umpvCGGJV2h23ZgP2JwjBqgpi4KZHfCq",
    );
}

#[test]
fn a_wallet_permit_signed_by_a_trading_key_is_refused() {
    assert_eq!(revoke_trading_key(&auth(TRADING), &Pubkey::from_literal(TRADING)), Err(BadPermit::NotOwner));
}

#[test]
fn trading_key_json_permits_match_the_docs_vectors() {
    let trading = auth(TRADING);
    let cases: [(Vec<u8>, &str, &str); 5] = [
        (
            generate_referral_code(&trading).unwrap(),
            r#"["generate_referral_code",1,3,5000,1731536000000,"GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB","2KW2XRd9kwqet15Aha2oK3tYvd3nWbTFH1MBiRAv1BE1"]"#,
            "4UpZqQfCNDbj3NEnkkRUQsW5USTTcv7JekvPWyHsEC2ifcc3R8PwEVd5xVWZuSNacyNHrZg29ZqPeUME3GWGpnCN",
        ),
        (
            claim_custom_referral_code(&trading, "trader1").unwrap(),
            r#"["claim_custom_referral_code","trader1",1,3,5000,1731536000000,"GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB","2KW2XRd9kwqet15Aha2oK3tYvd3nWbTFH1MBiRAv1BE1"]"#,
            "26FzDQPEDR6u7rKhkaEEJL1YSyKpbUCCibQtQxkDfEfEjyAsJu17X5p8TtuW5iXQfCNbs9gjEsFecqPv4yffwsgS",
        ),
        (
            apply_referral_code(&trading, "k7wq2m4x").unwrap(),
            r#"["apply_referral_code","k7wq2m4x",1,3,5000,1731536000000,"GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB","2KW2XRd9kwqet15Aha2oK3tYvd3nWbTFH1MBiRAv1BE1"]"#,
            "32Vj9xAGjh6xzfuDNqsWeAghnacBDFKQzwUWb5HgNeZBjw6tthL3iEpL91cW9iB8Vwv5PDNUSrUwH2gddH2je4MF",
        ),
        (
            set_username(&trading, "Trader").unwrap(),
            r#"["set_username","Trader",1,3,5000,1731536000000,"GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB","2KW2XRd9kwqet15Aha2oK3tYvd3nWbTFH1MBiRAv1BE1"]"#,
            "2aEt4QQeG3iFTD6tAZvDnfKzna7cnVh18eq6cveGho9mdwfDGLoFjs1fsyYieXuZKKGdpFFCH8VgvVQJ2K1BBmjV",
        ),
        (
            set_account_bio(&trading, "verify \"\u{1F642}\"").unwrap(),
            "[\"set_account_bio\",\"verify \\\"\u{1F642}\\\"\",1,3,5000,1731536000000,\"GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB\",\"2KW2XRd9kwqet15Aha2oK3tYvd3nWbTFH1MBiRAv1BE1\"]",
            "33NKyaFk1DPtmndQvJZCph639oYtVuPUjWieDHfVKD6SKmWd5sW87Qczai9tqSQUkinm1ankSwMPWrqY4kRbLr93",
        ),
    ];
    for (message, expected, signature) in cases {
        assert_eq!(text(&message), expected);
        signs(&trading_key(), &message, signature);
    }
}
