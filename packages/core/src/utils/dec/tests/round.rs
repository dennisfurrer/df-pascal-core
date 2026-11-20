//! Rounding to places, to significant figures and to a count of a smaller unit.

use super::*;

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"))
}

/// Down is toward zero, up away from it, and nearest away from zero on a tie. A chain amount in
/// millionths is an integer.
#[test]
fn rounding_goes_the_way_it_is_asked() {
    for (got, wanted) in [
        (dec("1.23456").round(2, Round::Down), "1.23"),
        (dec("1.23456").round(2, Round::Up), "1.24"),
        (dec("1.235").round(2, Round::Nearest), "1.24"),
        (dec("-1.235").round(2, Round::Nearest), "-1.24"),
        (dec("1.2").round(4, Round::Up), "1.2"),
        (dec("-1.239").round(2, Round::Down), "-1.23"),
        (dec("123456.7").round_sig(5, Round::Down), "123450"),
        (dec("0.001234567").round_sig(5, Round::Down), "0.0012345"),
        (dec("0.001234567").round_sig(5, Round::Up), "0.0012346"),
    ] {
        assert_eq!(got, dec(wanted), "{wanted}");
    }
    for (value, round, units) in
        [("12.5", Round::Down, 12_500_000), ("-0.0000015", Round::Down, -1), ("-0.0000015", Round::Up, -2)]
    {
        assert_eq!(dec(value).units_at(6, round), Some(units), "{value} {round:?}");
    }
}
