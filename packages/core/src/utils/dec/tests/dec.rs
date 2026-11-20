//! Equality and order across scales, and figures written into the code.

use super::*;

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"))
}

#[test]
fn a_value_is_the_same_at_any_scale_and_a_literal_is_its_units_at_its_scale() {
    assert_eq!(dec("1.50"), dec("1.5"));
    assert!(dec("0.99999") < dec("1"));
    assert!(dec("-2") < dec("-1.5"));
    assert_eq!(dec("30578.5").max(dec("30578.55")), dec("30578.55"));

    const TENTH: Dec = Dec::lit::<1>(1);
    const TICK: Dec = Dec::lit::<5>(1);
    const FINEST: Dec = Dec::lit::<MAX_SCALE>(-1);
    for (literal, text) in
        [(TENTH, "0.1"), (TICK, "0.00001"), (FINEST, "-0.000000000000000001"), (Dec::lit::<2>(475), "4.75")]
    {
        assert_eq!(literal, dec(text), "{text}");
    }
    assert_eq!(Dec::lit::<2>(1000).to_wire(), "10", "kept as it reads, written without trailing zeros");
    assert_eq!(Dec::lit::<0>(42), Dec::from_int(42));
    assert_eq!(Some(Dec::lit::<4>(5)), Dec::new(5, 4));
    assert_eq!(Dec::new(1, MAX_SCALE + 1), None, "past eighteen places");
}
