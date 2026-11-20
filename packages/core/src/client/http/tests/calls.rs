use super::*;

#[test]
fn a_success_envelope_reads_with_its_context() {
    let text = r#"{"status":"success","data":{"server_time_ms":"1731536000123"},"seq":"67891","state_time_ms":"1731536000050","server_time_ms":"1731536000123"}"#;
    let reply: Reply<serde_json::Value> = decode("/api/v1/time", 200, text).unwrap();
    assert_eq!(reply.context.seq, Some(67_891));
    assert_eq!(reply.context.state_time_ms, Some(1_731_536_000_050));
    assert_eq!(reply.data["server_time_ms"], "1731536000123");
}

#[test]
fn an_error_envelope_is_a_refusal_with_its_code_and_status() {
    let text = r#"{"status":"error","data":{"code":"HISTORY_NOT_READY","message":"later"},"server_time_ms":"1"}"#;
    let error = decode::<serde_json::Value>("/api/v1/trades", 503, text).unwrap_err();
    let ApiError::Refused(refusal) = &error else { panic!("{error:?}") };
    assert_eq!(refusal.code, ErrorCode::HistoryNotReady);
    assert_eq!(refusal.status, Some(503));
    assert!(refusal.code.is_retryable());
    assert!(!error.may_have_landed());
}

#[test]
fn a_page_that_is_not_an_envelope_is_a_status_error() {
    let error = decode::<serde_json::Value>("/x", 502, "<html>bad gateway</html>").unwrap_err();
    assert!(matches!(error, ApiError::Transport(TransportError::Status { status: 502, .. })));
    assert!(error.may_have_landed());
}

#[test]
fn a_batch_answers_each_request_in_order() {
    let text = r#"[{"status":"success","data":{"a":1},"seq":"1"},{"status":"error","data":{"code":"ORDER_NOT_FOUND","message":"gone"},"seq":"1"}]"#;
    let answers = decode_batch::<serde_json::Value>("/api/v1/cancels", 200, text).unwrap();
    assert_eq!(answers.len(), 2);
    assert!(answers[0].is_ok());
    assert_eq!(answers[1].as_ref().unwrap_err().code, ErrorCode::OrderNotFound);
}

#[test]
fn a_request_level_refusal_of_a_batch_is_one_error() {
    let text = r#"{"status":"error","data":{"code":"RATE_LIMITED","message":"slow down"}}"#;
    let error = decode_batch::<serde_json::Value>("/api/v1/orders", 429, text).unwrap_err();
    assert!(error.is_rate_limited());
}

#[test]
fn queries_are_percent_encoded_and_keep_commas() {
    let call = Call::get("/api/v1/trades")
        .field("symbols", "A.B,C.D")
        .field("before_cursor", "00000000000000067890:0000000000");
    assert_eq!(call.query_string(), "symbols=A.B,C.D&before_cursor=00000000000000067890%3A0000000000");
}
