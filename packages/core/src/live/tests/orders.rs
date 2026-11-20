use super::*;

#[test]
fn answers_read_by_kind_and_match_by_id() {
    let place = r#"{"id":"1","type":"place","payload":[{"status":"error","data":{"code":"WOULD_CROSS","message":"x"},"seq":"1"}]}"#;
    let OrderAnswer::Place { id, results } = decode_answer(place).unwrap() else { panic!() };
    assert_eq!(id.as_deref(), Some("1"));
    assert_eq!(results[0].as_ref().unwrap_err().code, ErrorCode::WouldCross);

    let rejected = r#"{"id":"3","type":"error","payload":{"data":{"code":"RATE_LIMITED","message":"rate limited; slow down before retrying"},"client_order_ids":["42"]}}"#;
    let answer = decode_answer(rejected).unwrap();
    assert_eq!(answer.id(), Some("3"));
    assert!(
        matches!(answer, OrderAnswer::Rejected { code: ErrorCode::RateLimited, ref client_order_ids, .. } if client_order_ids == &[42])
    );
    assert_eq!(decode_answer(r#"{"type":"pong"}"#).unwrap(), OrderAnswer::Pong);
}
