use super::*;

#[test]
fn an_unknown_value_is_kept_and_written_back() {
    let side: Side = serde_json::from_str("\"SIDEWAYS\"").unwrap();
    assert_eq!(side, Side::Unknown("SIDEWAYS".to_owned()));
    assert_eq!(serde_json::to_string(&side).unwrap(), "\"SIDEWAYS\"");
    assert_eq!(serde_json::from_str::<UpdateReason>("\"MASS_CANCELLED\"").unwrap(), UpdateReason::MassCancelled);
}

#[test]
fn input_is_read_strictly_whatever_its_case() {
    assert_eq!("bid".parse::<Side>(), Ok(Side::Bid));
    assert_eq!("1H".parse::<Interval>(), Ok(Interval::H1));
    assert!("sideways".parse::<Side>().is_err());
}

#[test]
fn permit_bytes() {
    assert_eq!(Side::Ask.byte(), Some(1));
    assert_eq!(TimeInForce::Ioc.byte(), Some(2));
    assert_eq!(OrderType::Market.byte(), Some(1));
    assert_eq!(Side::Unknown(String::new()).byte(), None);
    assert_eq!(Interval::D1.ms(), Some(86_400_000));
}
