//! Wire format parsing, rejected inputs and serde.

use super::*;

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"))
}

/// Read with an optional sign and at most one point; written with no trailing zeros and no
/// exponent. Anything else is refused.
#[test]
fn a_plain_decimal_reads_and_writes_back_without_trailing_zeros_and_nothing_else_reads() {
    for (text, wire) in [
        ("83982.0", Some("83982")),
        ("0.0000125", Some("0.0000125")),
        ("-0.0004701178", Some("-0.0004701178")),
        ("00123", Some("123")),
        ("1.2000", Some("1.2")),
        (".5", Some("0.5")),
        ("-0.0", Some("0")),
        ("2460964638.0624189377", Some("2460964638.0624189377")),
        ("", None),
        (".", None),
        ("1e5", None),
        ("1.2.3", None),
        ("abc", None),
        ("--1", None),
        ("1,5", None),
    ] {
        assert_eq!(Dec::parse(text).ok().map(Dec::to_wire).as_deref(), wire, "{text:?}");
    }
    assert!(matches!(Dec::parse("0.0000000000000000001"), Err(BadDec::TooFine(_))));
}

#[test]
fn the_venue_s_strings_read_and_floats_do_not() {
    let read: Vec<Dec> = serde_json::from_str(r#"["8155.50418728","0.0","41085992.9213469997",7]"#).unwrap();
    assert_eq!(read, [dec("8155.50418728"), Dec::ZERO, dec("41085992.9213469997"), dec("7")]);
    assert!(serde_json::from_str::<Dec>("1.5").is_err(), "a JSON fraction was a float first");
    assert!(serde_json::from_str::<Dec>(r#""1e5""#).is_err());
    assert_eq!(serde_json::to_string(&dec("12.50")).unwrap(), r#""12.5""#);
}
