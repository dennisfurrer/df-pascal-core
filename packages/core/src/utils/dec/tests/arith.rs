//! Exact sums, products and quotients, quotients of large figures that must not overflow, and
//! `None` where a figure would pass what a `Dec` holds.

use super::*;

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"))
}

/// Lending reserves in the hundreds of millions at ten places must not overflow an i128 on the
/// way to their ratio. Expected quotients are Python `decimal`'s at 80 digits.
#[test]
fn every_operation_is_exact_to_the_places_asked() {
    let (supplied, borrowed) = ("612345678.9876543211", "512345678.1234567891");
    let cases = [
        (dec("0.1").checked_add(dec("0.2")), Some("0.3")),
        (dec("83982").checked_mul(dec("1.03")), Some("86501.46")),
        (dec("100").checked_sub(dec("0.001")), Some("99.999")),
        (dec("10").checked_div(dec("3"), 4, Round::Down), Some("3.3333")),
        (dec("10").checked_div(dec("3"), 4, Round::Up), Some("3.3334")),
        (dec("2").checked_div(dec("3"), 2, Round::Nearest), Some("0.67")),
        (dec("1").checked_div(Dec::ZERO, 2, Round::Down), None),
        (dec(borrowed).checked_div(dec(supplied), 10, Round::Down), Some("0.8366935469")),
        (dec(borrowed).checked_div(dec(supplied), 10, Round::Up), Some("0.836693547")),
        (dec(borrowed).checked_div(dec(supplied), 10, Round::Nearest), Some("0.836693547")),
        (dec("-512345678.1234567891").checked_div(dec(supplied), 10, Round::Down), Some("-0.8366935469")),
        (dec("-512345678.1234567891").checked_div(dec(supplied), 10, Round::Up), Some("-0.836693547")),
        (
            dec("498765432.1234567891").checked_div(dec("734567890.9876543213"), 10, Round::Nearest),
            Some("0.6789916061"),
        ),
        (
            dec("98765432109876543.21").checked_div(dec("0.0000000003"), 2, Round::Down),
            Some("329218107032921810700000000"),
        ),
        (dec("1").checked_div(dec("7"), 18, Round::Down), Some("0.142857142857142857")),
        (dec("1").checked_div(dec("7"), 18, Round::Up), Some("0.142857142857142858")),
    ];
    for (at, (got, wanted)) in cases.into_iter().enumerate() {
        assert_eq!(got, wanted.map(dec), "case {at}");
    }
}

#[test]
fn a_sum_is_exact_and_none_past_what_a_dec_holds() {
    assert_eq!(Dec::checked_sum([dec("0.1"), dec("0.2"), dec("-0.05")]), Some(dec("0.25")));
    assert_eq!(Dec::checked_sum([]), Some(Dec::ZERO), "nothing sums to zero");
    assert_eq!(Dec::checked_sum(vec![dec("1.5")].into_iter()), Some(dec("1.5")));
    let huge = Dec::new(i128::MAX, 0).unwrap();
    assert_eq!(Dec::checked_sum([huge, Dec::ONE]), None);
    assert_eq!(Dec::checked_sum([huge, Dec::ONE, Dec::ONE.negate()]), None, "an overflow on the way is not undone");
}
