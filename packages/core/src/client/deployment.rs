//! Hosts, deployment id and USD mint for one Pascal deployment. The deployment id is signed into
//! every permit, so a request signed for one deployment cannot be replayed on another.

use crate::utils::keys::Pubkey;
use crate::utils::solana::{PASCAL_PROGRAM, USDC_MINT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
    pub name: String,
    /// Signed into every permit: local 0, staging 1, canary 2, prod 3.
    pub deployment_id: u8,
    /// Read API base URL: markets, books, accounts, history.
    pub read_url: String,
    /// Write API base URL, used for all signed requests.
    pub write_url: String,
    /// Market data websocket URL.
    pub data_ws_url: String,
    /// Order entry websocket URL.
    pub trade_ws_url: String,
    pub web_url: String,
    /// Mint of the token used for deposits and withdrawals.
    pub usd_mint: Pubkey,
    pub program: Pubkey,
}

impl Deployment {
    /// Production deployment (deployment id 3).
    #[must_use]
    pub fn prod() -> Deployment {
        Deployment {
            name: "prod".to_owned(),
            deployment_id: 3,
            read_url: "https://data.pascal.trade".to_owned(),
            write_url: "https://trade.pascal.trade".to_owned(),
            data_ws_url: "wss://data.pascal.trade/ws".to_owned(),
            trade_ws_url: "wss://trade.pascal.trade/ws".to_owned(),
            web_url: "https://app.pascal.trade".to_owned(),
            usd_mint: Pubkey::from_literal(USDC_MINT),
            program: Pubkey::from_literal(PASCAL_PROGRAM),
        }
    }

    /// Canary deployment (deployment id 2).
    #[must_use]
    pub fn canary() -> Deployment {
        Deployment {
            name: "canary".to_owned(),
            deployment_id: 2,
            read_url: "https://data.canary.pascal.trade".to_owned(),
            write_url: "https://trade.canary.pascal.trade".to_owned(),
            data_ws_url: "wss://data.canary.pascal.trade/ws".to_owned(),
            trade_ws_url: "wss://trade.canary.pascal.trade/ws".to_owned(),
            web_url: "https://app.canary.pascal.trade".to_owned(),
            ..Deployment::prod()
        }
    }

    /// Looks up `prod` or `canary` by name, ignoring case.
    #[must_use]
    pub fn named(name: &str) -> Option<Deployment> {
        match name.trim().to_ascii_lowercase().as_str() {
            "prod" | "production" | "" => Some(Deployment::prod()),
            "canary" => Some(Deployment::canary()),
            _ => None,
        }
    }

    /// Uses one root URL for every host, for a proxy or a test server.
    #[must_use]
    pub fn at(root: &str, deployment_id: u8) -> Deployment {
        let root = root.trim_end_matches('/');
        let ws = root.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1);
        Deployment {
            name: "custom".to_owned(),
            deployment_id,
            read_url: root.to_owned(),
            write_url: root.to_owned(),
            data_ws_url: format!("{ws}/ws"),
            trade_ws_url: format!("{ws}/ws"),
            web_url: root.to_owned(),
            ..Deployment::prod()
        }
    }
}

impl Default for Deployment {
    fn default() -> Self {
        Deployment::prod()
    }
}
