//! String enums used in Pascal requests and responses. An unrecognised value is read as
//! `Unknown(text)` instead of failing, and is written back unchanged.

/// An enum written as one of a fixed set of strings, with `Unknown` for any other.
macro_rules! wire_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($(#[$vmeta])* $variant,)+
            /// A value not known to this version, kept as received.
            Unknown(String),
        }

        impl $name {
            /// All known values.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The wire string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                match self {
                    $($name::$variant => $text,)+
                    $name::Unknown(text) => text.as_str(),
                }
            }

            /// Parses the wire string; an unknown value becomes `Unknown`.
            #[must_use]
            pub fn from_wire(text: &str) -> $name {
                match text {
                    $($text => $name::$variant,)+
                    other => $name::Unknown(other.to_owned()),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = crate::utils::order::UnknownValue;

            /// Case-insensitive parse for user input; an unknown value is an error.
            fn from_str(text: &str) -> Result<Self, Self::Err> {
                $(if text.eq_ignore_ascii_case($text) { return Ok($name::$variant); })+
                Err(crate::utils::order::UnknownValue { kind: stringify!($name), text: text.to_owned() })
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <std::borrow::Cow<'de, str> as serde::Deserialize>::deserialize(deserializer)?;
                Ok($name::from_wire(&text))
            }
        }
    };
}

pub(crate) use wire_enum;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{text}` is not a {kind}")]
pub struct UnknownValue {
    pub kind: &'static str,
    pub text: String,
}

wire_enum! {
    /// Buy (`BID`, long) or sell (`ASK`, short).
    Side { Bid = "BID", Ask = "ASK" }
}

impl Side {
    #[must_use]
    pub fn opposite(&self) -> Side {
        match self {
            Side::Bid => Side::Ask,
            Side::Ask => Side::Bid,
            Side::Unknown(text) => Side::Unknown(text.clone()),
        }
    }

    /// The byte a permit carries: 0 for a bid, 1 for an ask.
    #[must_use]
    pub fn byte(&self) -> Option<u8> {
        match self {
            Side::Bid => Some(0),
            Side::Ask => Some(1),
            Side::Unknown(_) => None,
        }
    }
}

wire_enum! {
    /// How long an order stays on the book.
    TimeInForce {
        /// Until filled or canceled.
        Gtc = "GTC",
        /// Until `expires_ts_ms`.
        Gtt = "GTT",
        /// Fills what it can immediately; the rest is canceled.
        Ioc = "IOC",
    }
}

impl TimeInForce {
    #[must_use]
    pub fn byte(&self) -> Option<u8> {
        match self {
            TimeInForce::Gtc => Some(0),
            TimeInForce::Gtt => Some(1),
            TimeInForce::Ioc => Some(2),
            TimeInForce::Unknown(_) => None,
        }
    }
}

wire_enum! {
    /// How an order is shown. A market order is an IOC order at a limit price.
    OrderType { Limit = "LIMIT", Market = "MARKET" }
}

impl OrderType {
    #[must_use]
    pub fn byte(&self) -> Option<u8> {
        match self {
            OrderType::Limit => Some(0),
            OrderType::Market => Some(1),
            OrderType::Unknown(_) => None,
        }
    }
}

wire_enum! {
    /// Whether a fill took liquidity or gave it.
    Liquidity { Taker = "TAKER", Maker = "MAKER" }
}

wire_enum! {
    /// Why an order changed outside a plain fill.
    UpdateReason {
        UserCancel = "USER_CANCEL",
        MassCancelled = "MASS_CANCELLED",
        Expired = "EXPIRED",
        Undercollateralized = "UNDERCOLLATERALIZED",
        SelfTrade = "SELF_TRADE",
        IocRemainderCanceled = "IOC_REMAINDER_CANCELED",
        MarketResolved = "MARKET_RESOLVED",
        PositionSizeLimit = "POSITION_SIZE_LIMIT",
        TradingKeyExpired = "TRADING_KEY_EXPIRED",
        TradingKeyRevoked = "TRADING_KEY_REVOKED",
        TradingKeyReplaced = "TRADING_KEY_REPLACED",
        NoReduciblePosition = "NO_REDUCIBLE_POSITION",
        ReduceOnlyResize = "REDUCE_ONLY_RESIZE",
        RestingOrderLimit = "RESTING_ORDER_LIMIT",
        Replaced = "REPLACED",
    }
}

wire_enum! {
    /// Status of a deposit, withdrawal or reward.
    TransferStatus { Verifying = "VERIFYING", Processing = "PROCESSING", Rejected = "REJECTED", Completed = "COMPLETED" }
}

wire_enum! {
    TransferType { Deposit = "DEPOSIT", Withdrawal = "WITHDRAWAL", Reward = "REWARD" }
}

wire_enum! {
    /// A closed position's direction.
    PositionSide { Long = "LONG", Short = "SHORT" }
}

wire_enum! {
    /// Candle interval.
    Interval {
        M1 = "1m",
        M5 = "5m",
        M15 = "15m",
        H1 = "1h",
        H4 = "4h",
        H12 = "12h",
        D1 = "1d",
    }
}

impl Interval {
    /// The interval in milliseconds.
    #[must_use]
    pub fn ms(&self) -> Option<u64> {
        const MINUTE: u64 = 60_000;
        Some(match self {
            Interval::M1 => MINUTE,
            Interval::M5 => 5 * MINUTE,
            Interval::M15 => 15 * MINUTE,
            Interval::H1 => 60 * MINUTE,
            Interval::H4 => 240 * MINUTE,
            Interval::H12 => 720 * MINUTE,
            Interval::D1 => 1_440 * MINUTE,
            Interval::Unknown(_) => return None,
        })
    }
}

wire_enum! {
    /// A leaderboard window.
    RankWindow { Day = "24h", Week = "7d", Month = "30d", All = "all" }
}

wire_enum! {
    /// A market category for leaderboard ranks.
    Category { Politics = "Politics", Geopolitics = "Geopolitics", Finance = "Finance", Tech = "Tech", Sports = "Sports" }
}

wire_enum! {
    /// What trader search orders by.
    RankBy { Pnl = "pnl", Volume = "volume" }
}

wire_enum! {
    /// An account history chart's range.
    Timeframe { Day = "day", Week = "week", Month = "month", AllTime = "all_time" }
}

wire_enum! {
    /// A PnL movers window.
    MoversWindow { Week = "7d", Day = "24h", HalfDay = "12h", SixHours = "6h" }
}

wire_enum! {
    MoversOrderBy { AbsPnlChange = "abs_pnl_change", PnlChange = "pnl_change" }
}

wire_enum! {
    SortOrder { Asc = "asc", Desc = "desc" }
}

wire_enum! {
    /// What closed position history orders by.
    PositionOrderBy { CloseTime = "close_time", RealizedPnl = "realized_pnl" }
}

wire_enum! {
    /// How far back resolved markets go.
    Lookback { H4 = "4h", H24 = "24h", D3 = "3d" }
}

wire_enum! {
    /// What resolved markets are ordered by.
    ResolvedOrderBy { ResolutionTime = "resolution_time", Volume24h = "volume_24h", VolumeAllTime = "volume_all_time" }
}

wire_enum! {
    /// Which markets a text search returns.
    MarketStatus { Live = "live", Resolved = "resolved", All = "all" }
}

wire_enum! {
    /// What a text search orders by.
    SearchOrderBy { Volume24h = "volume_24h", VolumeAllTime = "volume_all_time" }
}

wire_enum! {
    /// A market's default sort on the web app.
    MarketSort {
        Price = "PRICE",
        SymbolAsc = "SYMBOL_ASC",
        SymbolDesc = "SYMBOL_DESC",
        MarketDescriptionAsc = "MARKET_DESCRIPTION_ASC",
        MarketDescriptionDesc = "MARKET_DESCRIPTION_DESC",
        Volume = "VOLUME",
        ExpectedResolutionTime = "EXPECTED_RESOLUTION_TIME",
    }
}

wire_enum! {
    /// Why a market was sent on the `market_update` channel.
    MarketUpdateType {
        New = "new",
        DisplayAttributesChange = "display_attributes_change",
        MeceDesignation = "mece_designation",
        Resolved = "resolved",
    }
}

#[cfg(test)]
#[path = "tests/order.rs"]
mod tests;
