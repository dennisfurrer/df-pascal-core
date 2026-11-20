//! Client tests against a fake server that returns scripted responses and records every request.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::markets;
use crate::trade::{CancelBy, OrderRequest};
use crate::utils::dec::Dec;
use crate::utils::order::Side;
use crate::utils::sign::Keypair;

#[derive(Clone, Default)]
struct Venue {
    sent: Rc<RefCell<Vec<Request>>>,
    answers: Rc<RefCell<Vec<(u16, String)>>>,
}

impl Venue {
    fn answer(&self, status: u16, body: &str) {
        self.answers.borrow_mut().push((status, body.to_owned()));
    }

    fn last(&self) -> Request {
        self.sent.borrow().last().cloned().unwrap()
    }
}

impl Http for Venue {
    async fn send(&self, request: Request) -> Result<Response, TransportError> {
        request.refuse_if_offline()?;
        self.sent.borrow_mut().push(request);
        let (status, body) = self.answers.borrow_mut().remove(0);
        Ok(Response { status, headers: vec![("Retry-After".to_owned(), "2".to_owned())], body: body.into_bytes() })
    }
}

fn clock() -> u64 {
    1_731_536_000_000
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)
}

fn trader(venue: &Venue) -> Client<Venue, Trader<Keypair>> {
    let owner = Keypair::from_seed(&[7; 32]).pubkey();
    Client::new(venue.clone(), clock, Trader::new(owner, Keypair::from_seed(&[8; 32])))
}

const PLACED: &str = r#"[{"status":"success","data":{"order":{"id":"987654321","client_order_id":"42","symbol":"SIM_EVENT_1.MARKET_1","side":"BID","price":"0.550000","size_original":"10","size_remaining":"7","size_filled":"3","notional_filled":"1.650000","tif":"GTC","type":"LIMIT","post_only":false,"reduce_only":false,"seq":"67880","update_seq":"67891","place_ts_ms":"1731536000050","update_ts_ms":"1731536000050"},"fills":[{"trade_id":"555001","symbol":"SIM_EVENT_1.MARKET_1","side":"BID","reduce_only":false,"fill_price":"0.550000","fill_size":"3","liquidity":"TAKER","counterparty":"5Z6Ay5NEcbg3xhopc522sBCRXQujkTiuDRnHGfQdcnSf","order_id":"987654321","client_order_id":"42","order_progress":{"size_original":"10","size_filled":"3","size_remaining":"7"},"fee_usd":"0.000742","collateral_change_usd":"-1.650742","position_size_prev":"0","realized_pnl_usd":"0.000000","seq":"67891","trade_ts_ms":"1731536000050"}]},"seq":"67891","state_time_ms":"1731536000050","server_time_ms":"1731536000123"}]"#;

#[test]
fn an_order_goes_out_as_the_docs_request_and_its_answer_reads() {
    let venue = Venue::default();
    let client = trader(&venue);
    venue.answer(200, PLACED);
    let order = OrderRequest::limit("SIM_EVENT_1.MARKET_1", Side::Bid, Dec::parse("0.55").unwrap(), 10, 42);
    let placed = block_on(crate::trade::place(&client, &order)).unwrap();
    assert_eq!(placed.data.order.size_filled, 3);
    assert_eq!(placed.data.fills[0].position_size_after(), 3);
    assert_eq!(placed.data.order.average_fill_price(), Some(Dec::parse("0.55").unwrap()));

    let request = venue.last();
    assert_eq!(request.url, "https://trade.pascal.trade/api/v1/orders");
    assert!(request.writes);
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    // The docs' example request, signed with the client's clock.
    assert_eq!(body[0]["client_order_id"], "42");
    assert_eq!(body[0]["price"], "0.550000");
    assert_eq!(body[0]["size"], "10");
    assert_eq!(body[0]["auth"]["client_ts_ms"], "1731536000000");
    assert_eq!(body[0]["auth"]["signer"], "2KW2XRd9kwqet15Aha2oK3tYvd3nWbTFH1MBiRAv1BE1");
    assert_eq!(
        body[0]["auth"]["signature"],
        "2rUctQ9bFbuQaUxbXDGRtGm4LMj3GdbYWAW6pjrznAsLvpdH8Eqyx3LgsFx3YzV9cvMfw1ND5T5nChUQZZJ7jEad"
    );
    assert!(body[0].get("tif").is_none() && body[0].get("metadata").is_none());
}

#[test]
fn timestamps_rise_so_a_cancel_after_an_order_never_shares_one() {
    let venue = Venue::default();
    let client = trader(&venue);
    let first = crate::trade::cancel::sign_cancel(&client, CancelBy::ClientOrderId(42)).unwrap();
    let second = crate::trade::cancel::sign_cancel(&client, CancelBy::ClientOrderId(43)).unwrap();
    assert_eq!(first.auth.client_ts_ms, 1_731_536_000_000);
    assert_eq!(second.auth.client_ts_ms, 1_731_536_000_001);
    // The docs' first cancel signature, for the docs' timestamp.
    assert_eq!(
        first.signature.to_base58(),
        "591YziUjZzro24mamxgRrSDdYD71NpsDBmU4UGSyrBYbFiWzYaLjR9he1P5DP8djMKMJzHTsZzwRAagvLqgtiXnc"
    );
}

#[test]
fn a_reader_refuses_a_trading_request_without_sending_it() {
    let venue = Venue::default();
    let client = Client::reader(venue.clone(), clock).with_key(());
    let order = OrderRequest::limit("A.B", Side::Bid, Dec::parse("0.5").unwrap(), 1, 1);
    let error = crate::trade::orders::sign_place(&client, &order).unwrap_err();
    assert!(matches!(error, ApiError::Transport(TransportError::NoKey(_))));
    assert!(venue.sent.borrow().is_empty());
}

#[test]
fn a_wallet_request_takes_a_signature_from_outside_only_when_it_verifies() {
    let venue = Venue::default();
    let client = Client::reader(venue.clone(), clock);
    let wallet = Keypair::from_seed(&[7; 32]);
    let trading = Keypair::from_seed(&[8; 32]).pubkey();
    let prepared = client.prepare_trading_key(wallet.pubkey(), &trading, "api-doc-key", 7).unwrap();
    assert!(prepared.message_text().unwrap().starts_with("[\"create_trading_key\""));
    let wrong = Keypair::from_seed(&[9; 32]).sign(prepared.message());
    assert!(prepared.clone().with_signature(wrong).is_err());
    let signed = prepared.clone().with_signature(wallet.sign(prepared.message())).unwrap();
    assert!(prepared.sign(&Keypair::from_seed(&[9; 32])).is_err());
    venue.answer(200, r#"{"status":"success","data":{"trading_key":"2KW2XRd9kwqet15Aha2oK3tYvd3nWbTFH1MBiRAv1BE1","name":"api-doc-key","expiration_ts_ms":"1732140800000","open_order_count":"0"}}"#);
    let key = block_on(client.send(&signed)).unwrap();
    assert_eq!(key.data.name, "api-doc-key");
    let body: serde_json::Value = serde_json::from_slice(&venue.last().body).unwrap();
    assert_eq!(body["trading_key"], "2KW2XRd9kwqet15Aha2oK3tYvd3nWbTFH1MBiRAv1BE1");
    // Seven days after the docs' timestamp equals the docs' expiry, so this is the docs' request.
    assert_eq!(body["expiration_ts_ms"], "1732140800000");
    assert_eq!(
        body["auth"]["signature"],
        "kyc2KdqPTeEWqoK3pXR2rgPUiK4zXg9QdQffPUWk8eALpFy7uaHBEsASt7RQyVzDeubHVsPqQAHXrt8m8KZ78Ep"
    );
}

#[test]
fn a_429_closes_the_gate_and_holds_the_next_request() {
    let venue = Venue::default();
    let client = Client::reader(venue.clone(), clock);
    venue.answer(429, r#"{"status":"error","data":{"code":"RATE_LIMITED","message":"slow"}}"#);
    let error = block_on(markets::server_time(&client)).unwrap_err();
    assert!(error.is_rate_limited());
    let held = block_on(markets::server_time(&client)).unwrap_err();
    assert!(matches!(held, ApiError::Transport(TransportError::RateLimited { retry_after_ms: Some(2_000) })));
    assert_eq!(venue.sent.borrow().len(), 1);
}

#[test]
fn the_withdrawal_pays_the_destination_s_usdc_account_and_signs_the_docs_payload() {
    let venue = Venue::default();
    let client = Client::reader(venue.clone(), clock);
    let wallet = Keypair::from_seed(&[7; 32]);
    let destination = Pubkey::from_literal("29d2S7vB453rNYFdR5Ycwt7y9haRT5fwVwL9zTmBhfV2");
    let prepared = client.prepare_withdrawal(wallet.pubkey(), Dec::parse("25").unwrap(), &destination).unwrap();
    assert_eq!(prepared.body.destination_token_account, crate::utils::solana::usdc_account(&destination));
    assert_eq!(prepared.body.amount, "25.000000");
    // The docs' test vector uses a token account under a test mint; with that account the
    // signature matches the docs.
    let docs = crate::transfer::prepare_withdrawal_to_account(
        client.wallet_auth(wallet.pubkey()),
        Dec::parse("25").unwrap(),
        &destination,
        &Pubkey::from_literal("3JF3sEqM796hk5WFqA6EtmEwJQ9quALszsfJyvXNQKy3"),
    )
    .unwrap();
    let signed = docs.sign(&wallet).unwrap();
    assert_eq!(signed.auth.client_ts_ms, 1_731_536_000_001);
    let at_docs_time = crate::transfer::prepare_withdrawal_to_account(
        crate::utils::permit::Auth { client_ts_ms: 1_731_536_000_000, ..signed.auth },
        Dec::parse("25").unwrap(),
        &destination,
        &Pubkey::from_literal("3JF3sEqM796hk5WFqA6EtmEwJQ9quALszsfJyvXNQKy3"),
    )
    .unwrap()
    .sign(&wallet)
    .unwrap();
    assert_eq!(
        at_docs_time.signature.to_base58(),
        "61YYUBk6BTXkfrksMbqDLUNxwo6jhMPqdP7fNnWjNF2FYvMDujTsaUxfCiRxXX72jxy4oLtt6AtLodCvjDi4XpDn"
    );
}

#[test]
fn reads_hit_the_read_host_with_their_query() {
    let venue = Venue::default();
    let client = Client::reader(venue.clone(), clock);
    venue.answer(200, r#"{"status":"success","data":{"items":[],"next_cursor":"c1"},"seq":"5"}"#);
    let page = block_on(markets::trades(&client, &["A.B"], &crate::utils::PageRequest::limit(5))).unwrap();
    assert_eq!(page.data.next_cursor.as_deref(), Some("c1"));
    assert_eq!(venue.last().url, "https://data.pascal.trade/api/v1/trades?symbols=A.B&limit=5");
}
