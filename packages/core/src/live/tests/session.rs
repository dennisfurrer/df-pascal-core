use super::*;

#[test]
fn wanted_channels_are_subscribed_on_open_and_kept_across_reconnects() {
    let mut session = Session::new("wss://data.pascal.trade/ws");
    session.subscribe(Channel::book("A.B")).unwrap();
    session.subscribe(Channel::AllTrades).unwrap();
    session.subscribe(Channel::book("A.B")).unwrap();
    assert!(session.owed(0).is_empty());
    let frames = session.opened(1_000);
    assert_eq!(frames.len(), 1);
    assert!(frames[0].starts_with(r#"{"type":"subscribe","channels":["#));
    assert!(frames[0].contains(r#"{"channel":"book","symbol":"A.B"}"#));
    assert!(frames[0].contains(r#"{"channel":"all_trades"}"#));

    session.unsubscribe(&Channel::AllTrades);
    assert_eq!(session.owed(1_001), vec![r#"{"type":"unsubscribe","channels":[{"channel":"all_trades"}]}"#.to_owned()]);

    let retry = session.closed(2_000, 0);
    assert_eq!(retry, Retry { after_ms: FIRST_RETRY_MS, attempt: 1 });
    assert_eq!(session.closed(2_500, 0).after_ms, 2 * FIRST_RETRY_MS);
    assert_eq!(session.opened(3_000).len(), 1);
}

#[test]
fn it_pings_when_quiet_and_reconnects_when_silent() {
    let mut session = Session::new("x");
    session.opened(0);
    assert_eq!(session.tick(1_000), Tick::Wait);
    assert_eq!(session.tick(PING_EVERY_MS), Tick::Send);
    assert_eq!(session.owed(PING_EVERY_MS), vec![channel::PING.to_owned()]);
    assert_eq!(session.tick(SILENCE_MS), Tick::Reconnect);
    let _ = session.received(SILENCE_MS, r#"{"type":"pong"}"#);
    assert_ne!(session.tick(SILENCE_MS + 1), Tick::Reconnect);
}

#[test]
fn every_channel_of_the_docs_reads() {
    let frames = [
        r#"{"channel":"book","symbol":"SIM_EVENT_1.MARKET_1","type":"snapshot","data":{"asks":[["0.560000","12"],["0.570000","20"]],"bids":[["0.540000","15"],["0.530000","7"]],"spec":{"symbol":"SIM_EVENT_1.MARKET_1","taker_fee_rate":"0.001000","maker_rebate_share":"0.500000","tick_size_min":"0.010000","tick_sig_figs":2}},"seq":"67891","state_time_ms":"1731536000050","server_time_ms":"1731536000123"}"#,
        r#"{"channel":"trades","symbol":"SIM_EVENT_1.MARKET_1","type":"update","data":[{"trade_id":"555001","symbol":"SIM_EVENT_1.MARKET_1","taker_side":"BID","price":"0.550000","size":"3","maker":"5Z6Ay5NEcbg3xhopc522sBCRXQujkTiuDRnHGfQdcnSf","taker":"GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB","seq":"67891","trade_ts_ms":"1731536000050"}],"seq":"67891"}"#,
        r#"{"channel":"all_trades","type":"snapshot","data":[],"seq":"1"}"#,
        r#"{"channel":"candles","symbol":"S.M","interval":"1m","type":"update","data":[{"time":"1731535960000","open":"0.520000","high":"0.570000","low":"0.510000","close":"0.550000","volume":"120","taker_buy_volume":"70","taker_sell_volume":"50","volume_notional":"66.000000","taker_buy_volume_notional":"38.500000","taker_sell_volume_notional":"27.500000","trade_count":"8","taker_buy_count":"5","taker_sell_count":"3"}]}"#,
        r#"{"channel":"mark_price_candles","symbol":"S.M","interval":"1m","type":"update","data":[{"time":"1731535960000","open":"0.540000","high":"0.580000","low":"0.530000","close":"0.560000"}]}"#,
        r#"{"channel":"account","owner":"GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB","type":"update","data":{"orders":[],"positions":[],"pending_deposits":[],"pending_withdrawals":[],"order_updates":[{"order":{"id":"987654322","client_order_id":"43","symbol":"S.M","side":"ASK","price":"0.550000","size_original":"10","size_remaining":"0","size_filled":"0","notional_filled":"0.000000","tif":"GTC","type":"LIMIT","post_only":false,"reduce_only":true,"seq":"67880","update_seq":"67891","place_ts_ms":"1731536000050","update_ts_ms":"1731536000050"},"update_reason":"NO_REDUCIBLE_POSITION"}]},"seq":"67891"}"#,
        r#"{"channel":"market_update","type":"snapshot","data":[],"seq":"67891"}"#,
    ];
    let mut session = Session::new("x");
    for text in frames {
        let frame = session.received(0, text).unwrap_or_else(|error| panic!("{error}"));
        assert!(matches!(frame, Frame::Message { .. }), "{text}");
    }
    let error = session
        .received(0, r#"{"channel":"book","symbol":"S.M","type":"error","data":{"code":"SYMBOL_NOT_FOUND","message":"symbol not found"},"seq":"1"}"#)
        .unwrap();
    assert!(matches!(
        error,
        Frame::Error { code: crate::client::ErrorCode::SymbolNotFound, channel: Some(Channel::Book { .. }), .. }
    ));
    assert_eq!(session.received(0, r#"{"type":"pong"}"#).unwrap(), Frame::Pong);
}
