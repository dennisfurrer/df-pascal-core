use super::*;

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn the_tick_follows_the_docs_table() {
    let min = dec("0.001");
    for (price, tick) in [
        ("0.50", "0.01"),
        ("0.10", "0.01"),
        ("0.90", "0.01"),
        ("0.85", "0.01"),
        ("0.025", "0.001"),
        ("0.975", "0.001"),
        ("0.099", "0.001"),
        ("0.001", "0.001"),
        ("0.997", "0.001"),
        ("0", "0.001"),
        ("1", "0.001"),
    ] {
        assert_eq!(tick_size(dec(price), min, 2), dec(tick), "{price}");
    }
    // Three significant figures with a finer floor.
    assert_eq!(tick_size(dec("0.25"), dec("0.0001"), 3), dec("0.001"));
    assert_eq!(tick_size(dec("0.0025"), dec("0.000001"), 2), dec("0.0001"));
}

#[test]
fn prices_round_onto_the_grid() {
    let min = dec("0.001");
    assert!(is_on_tick(dec("0.51"), min, 2));
    assert!(!is_on_tick(dec("0.515"), min, 2));
    assert!(is_on_tick(dec("0.043"), min, 2));
    assert_eq!(round_to_tick(dec("0.515"), min, 2, Round::Down), dec("0.51"));
    assert_eq!(round_to_tick(dec("0.515"), min, 2, Round::Up), dec("0.52"));
    assert_eq!(round_to_tick(dec("1.2"), min, 2, Round::Up), dec("1"));
    assert_eq!(step(dec("0.10"), min, 2, false), dec("0.099"));
    assert_eq!(step(dec("0.099"), min, 2, true), dec("0.1"));
    assert_eq!(step(dec("0.50"), min, 2, true), dec("0.51"));
    assert_eq!(step(dec("0.90"), min, 2, true), dec("0.901"));
    assert_eq!(step(dec("0.90"), min, 2, false), dec("0.89"));
    assert_eq!(step(dec("0.001"), min, 2, false), dec("0"));
}

#[test]
fn fees_follow_the_docs_examples() {
    // 100 contracts at $0.40: 24 weighted contracts.
    assert_eq!(weighted(100, dec("0.40")), dec("24"));
    // A 1% builder fee on 100 contracts at $0.50 pays $0.25.
    assert_eq!(builder_fee(100, dec("0.50"), dec("0.01")), dec("0.25"));
    // The docs' fill: 3 at $0.55 with a 0.1% taker rate.
    assert_eq!(taker_fee(3, dec("0.55"), dec("0.001")), dec("0.000743"));
    assert_eq!(maker_rebate(100, dec("0.5"), dec("0.02"), dec("0.25")), dec("0.125"));
}

#[test]
fn collateral_and_yes_no_follow_the_docs() {
    assert_eq!(collateral(&Side::Bid, 10, dec("0.30")), dec("3"));
    assert_eq!(collateral(&Side::Ask, 10, dec("0.30")), dec("7"));
    assert_eq!(yes_no(Outcome::Yes, true, dec("0.3")), (Side::Bid, dec("0.3"), false));
    assert_eq!(yes_no(Outcome::Yes, false, dec("0.3")), (Side::Ask, dec("0.3"), true));
    assert_eq!(yes_no(Outcome::No, true, dec("0.3")), (Side::Ask, dec("0.7"), false));
    assert_eq!(yes_no(Outcome::No, false, dec("0.3")), (Side::Bid, dec("0.7"), true));
}
