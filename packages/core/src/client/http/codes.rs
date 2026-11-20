//! Pascal's error codes, with helpers for whether a request can be retried and whether a write
//! may have been executed.

crate::utils::order::wire_enum! {
    /// `data.code` on an error envelope.
    ErrorCode {
        InvalidJson = "INVALID_JSON",
        InvalidRequest = "INVALID_REQUEST",
        Unauthorized = "UNAUTHORIZED",
        SymbolNotFound = "SYMBOL_NOT_FOUND",
        AccountNotFound = "ACCOUNT_NOT_FOUND",
        AccountRestricted = "ACCOUNT_RESTRICTED",
        GeoRestricted = "GEO_RESTRICTED",
        WithdrawalAddressNotAllowlisted = "WITHDRAWAL_ADDRESS_NOT_ALLOWLISTED",
        InvalidGeoHeaders = "INVALID_GEO_HEADERS",
        OrderNotFound = "ORDER_NOT_FOUND",
        TradingKeyNotFound = "TRADING_KEY_NOT_FOUND",
        DuplicateRequest = "DUPLICATE_REQUEST",
        DuplicateClientOrderId = "DUPLICATE_CLIENT_ORDER_ID",
        UnsupportedOrderType = "UNSUPPORTED_ORDER_TYPE",
        InvalidSize = "INVALID_SIZE",
        InvalidPrice = "INVALID_PRICE",
        InvalidExpiration = "INVALID_EXPIRATION",
        WouldCross = "WOULD_CROSS",
        SelfTrade = "SELF_TRADE",
        InsufficientCollateral = "INSUFFICIENT_COLLATERAL",
        NotFunded = "NOT_FUNDED",
        DepositAddressNotRegistered = "DEPOSIT_ADDRESS_NOT_REGISTERED",
        MarketResolved = "MARKET_RESOLVED",
        PositionSizeLimitExceeded = "POSITION_SIZE_LIMIT_EXCEEDED",
        RestingOrderLimitExceeded = "RESTING_ORDER_LIMIT_EXCEEDED",
        TradingTemporarilyUnavailable = "TRADING_TEMPORARILY_UNAVAILABLE",
        TradingKeySlotsFull = "TRADING_KEY_SLOTS_FULL",
        TradingKeyPubKeyAlreadyUsed = "TRADING_KEY_PUB_KEY_ALREADY_USED",
        TradingKeyNameAlreadyUsed = "TRADING_KEY_NAME_ALREADY_USED",
        TradingKeyMustOverwriteWithGreaterExpiration = "TRADING_KEY_MUST_OVERWRITE_WITH_GREATER_EXPIRATION",
        ReduceOnlyRejected = "REDUCE_ONLY_REJECTED",
        PendingWalletOpConflicts = "PENDING_WALLET_OP_CONFLICTS",
        PendingDepositRegistration = "PENDING_DEPOSIT_REGISTRATION",
        ReplacementOrderNotFound = "REPLACEMENT_ORDER_NOT_FOUND",
        ReplacementOrderDiffMarket = "REPLACEMENT_ORDER_DIFF_MARKET",
        ReplacementOrderDiffSide = "REPLACEMENT_ORDER_DIFF_SIDE",
        RecvWindowTooLarge = "RECV_WINDOW_TOO_LARGE",
        ClientTsTooFarInFuture = "CLIENT_TS_TOO_FAR_IN_FUTURE",
        ClientTsExpired = "CLIENT_TS_EXPIRED",
        ClientTsNotIncreasing = "CLIENT_TS_NOT_INCREASING",
        Conflict = "CONFLICT",
        RateLimited = "RATE_LIMITED",
        Backpressure = "BACKPRESSURE",
        NotReady = "NOT_READY",
        HistoryNotReady = "HISTORY_NOT_READY",
        BadGateway = "BAD_GATEWAY",
        ServiceUnavailable = "SERVICE_UNAVAILABLE",
        WriteResultUnavailable = "WRITE_RESULT_UNAVAILABLE",
        Timeout = "TIMEOUT",
        InternalError = "INTERNAL_ERROR",
        ReferralCodeInvalid = "REFERRAL_CODE_INVALID",
        BuilderFeeRateInvalid = "BUILDER_FEE_RATE_INVALID",
        BuilderInvalid = "BUILDER_INVALID",
        BuilderCollateralBelowMinimum = "BUILDER_COLLATERAL_BELOW_MINIMUM",
    }
}

impl ErrorCode {
    /// Pascal rejected the request before processing it because of load or readiness. Wait and
    /// send it again; a write must be signed again with a new timestamp.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ErrorCode::RateLimited
                | ErrorCode::Backpressure
                | ErrorCode::NotReady
                | ErrorCode::HistoryNotReady
                | ErrorCode::BadGateway
                | ErrorCode::ServiceUnavailable
        )
    }

    /// The write was accepted but its result was not returned, so it may have been executed.
    /// Read the account state before sending it again.
    #[must_use]
    pub fn may_have_landed(&self) -> bool {
        matches!(self, ErrorCode::WriteResultUnavailable | ErrorCode::Timeout | ErrorCode::InternalError)
    }

    /// The timestamp was rejected: the local clock is wrong or the request was sent too late.
    #[must_use]
    pub fn is_clock(&self) -> bool {
        matches!(
            self,
            ErrorCode::ClientTsTooFarInFuture | ErrorCode::ClientTsExpired | ErrorCode::ClientTsNotIncreasing
        )
    }
}
