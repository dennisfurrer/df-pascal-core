//! Solana JSON-RPC calls for deposits and withdrawal checks, sent over the client's [`Http`]
//! transport with `confirmed` commitment. Requests are not retried, and the public endpoint is
//! heavily rate limited.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::client::{Http, Request, TransportError};
use crate::utils::base64;
use crate::utils::keys::{Pubkey, Signature};
use crate::utils::{base58, lenient};

/// Solana's public mainnet endpoint. It refuses requests sent from browsers (403).
pub const MAINNET: &str = "https://api.mainnet-beta.solana.com";

/// PublicNode's free mainnet endpoint: no API key, answers browsers (CORS), and serves every call
/// here. It refuses `getTokenAccountBalance`, so [`Rpc::token_balance`] reads accounts instead.
pub const PUBLICNODE: &str = "https://solana-rpc.publicnode.com";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RpcError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("the node refused {method} ({code}): {message}")]
    Refused { method: &'static str, code: i64, message: String },
    #[error("the node's answer to {method} could not be read: {detail}")]
    Unreadable { method: &'static str, detail: String },
}

/// A Solana JSON-RPC endpoint.
#[derive(Debug, Clone)]
pub struct Rpc<H> {
    http: H,
    url: String,
}

/// A blockhash and the last block height at which a transaction using it is valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blockhash {
    pub hash: [u8; 32],
    pub last_valid_block_height: u64,
}

/// A token account's balance in base units, and the mint's decimals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenBalance {
    pub amount: u64,
    pub decimals: u8,
}

/// Status of a sent transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// `processed`, `confirmed` or `finalized`.
    pub confirmation: String,
    /// The program error, when it failed.
    pub error: Option<String>,
}

impl Status {
    /// True when confirmed or finalized.
    #[must_use]
    pub fn is_confirmed(&self) -> bool {
        matches!(self.confirmation.as_str(), "confirmed" | "finalized")
    }
}

impl<H: Http> Rpc<H> {
    pub fn new(http: H, url: impl Into<String>) -> Rpc<H> {
        Rpc { http, url: url.into() }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    async fn call<R: DeserializeOwned>(&self, method: &'static str, params: Value) -> Result<R, RpcError> {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
        let response = self.http.send(Request::post_json(&self.url, body)).await?;
        let text = response.text();
        #[derive(Deserialize)]
        struct Answer<R> {
            result: Option<R>,
            error: Option<Refusal>,
        }
        #[derive(Deserialize)]
        struct Refusal {
            #[serde(default)]
            code: i64,
            #[serde(default)]
            message: String,
        }
        let answer: Answer<R> = serde_json::from_str(&text).map_err(|error| match response.is_success() {
            true => RpcError::Unreadable { method, detail: error.to_string() },
            false => RpcError::Transport(TransportError::Status { status: response.status, body: text.clone() }),
        })?;
        match (answer.result, answer.error) {
            (_, Some(refusal)) => Err(RpcError::Refused { method, code: refusal.code, message: refusal.message }),
            (Some(result), None) => Ok(result),
            (None, None) => Err(RpcError::Unreadable { method, detail: "no result".to_owned() }),
        }
    }

    pub async fn latest_blockhash(&self) -> Result<Blockhash, RpcError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Value_ {
            blockhash: String,
            last_valid_block_height: u64,
        }
        #[derive(Deserialize)]
        struct Wrapped {
            value: Value_,
        }
        let answer: Wrapped = self.call("getLatestBlockhash", json!([{ "commitment": "confirmed" }])).await?;
        let hash = base58::decode_array::<32>(&answer.value.blockhash)
            .map_err(|error| RpcError::Unreadable { method: "getLatestBlockhash", detail: error.to_string() })?;
        Ok(Blockhash { hash, last_valid_block_height: answer.value.last_valid_block_height })
    }

    /// Whether `account` exists: a token account that was never created does not.
    pub async fn account_exists(&self, account: &Pubkey) -> Result<bool, RpcError> {
        #[derive(Deserialize)]
        struct Wrapped {
            value: Option<Value>,
        }
        let answer: Wrapped = self
            .call("getAccountInfo", json!([account.to_base58(), { "commitment": "confirmed", "encoding": "base64" }]))
            .await?;
        Ok(answer.value.is_some())
    }

    /// An account's data; `None` when the account does not exist.
    pub async fn account_data(&self, account: &Pubkey) -> Result<Option<Vec<u8>>, RpcError> {
        #[derive(Deserialize)]
        struct Info {
            data: (String, String),
        }
        #[derive(Deserialize)]
        struct Wrapped {
            value: Option<Info>,
        }
        let answer: Wrapped = self
            .call("getAccountInfo", json!([account.to_base58(), { "commitment": "confirmed", "encoding": "base64" }]))
            .await?;
        answer
            .value
            .map(|info| {
                base64::decode(&info.data.0)
                    .map_err(|error| RpcError::Unreadable { method: "getAccountInfo", detail: error.to_string() })
            })
            .transpose()
    }

    /// The token account's balance; `None` when it does not exist. Reads the token account and its
    /// mint with `getAccountInfo`, which public endpoints serve without an API key, unlike
    /// `getTokenAccountBalance`.
    pub async fn token_balance(&self, account: &Pubkey) -> Result<Option<TokenBalance>, RpcError> {
        let Some(data) = self.account_data(account).await? else { return Ok(None) };
        let unreadable = |detail: &str| RpcError::Unreadable { method: "getAccountInfo", detail: detail.to_owned() };
        let (mint, amount) = token_account(&data).ok_or_else(|| unreadable("not a token account"))?;
        let mint_data = self.account_data(&mint).await?.ok_or_else(|| unreadable("the mint does not exist"))?;
        let decimals = mint_decimals(&mint_data).ok_or_else(|| unreadable("not a mint"))?;
        Ok(Some(TokenBalance { amount, decimals }))
    }

    /// Balance in lamports.
    pub async fn sol_balance(&self, account: &Pubkey) -> Result<u64, RpcError> {
        #[derive(Deserialize)]
        struct Wrapped {
            value: u64,
        }
        let answer: Wrapped =
            self.call("getBalance", json!([account.to_base58(), { "commitment": "confirmed" }])).await?;
        Ok(answer.value)
    }

    /// Sends a signed transaction; its first signature is its id.
    pub async fn send_transaction(&self, transaction: &[u8]) -> Result<Signature, RpcError> {
        let id: String = self
            .call(
                "sendTransaction",
                json!([base64::encode(transaction), { "encoding": "base64", "preflightCommitment": "confirmed" }]),
            )
            .await?;
        Signature::parse(&id)
            .map_err(|error| RpcError::Unreadable { method: "sendTransaction", detail: error.to_string() })
    }

    /// Status of `signature`; `None` if the node has not seen it.
    pub async fn signature_status(&self, signature: &Signature) -> Result<Option<Status>, RpcError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct One {
            #[serde(default, deserialize_with = "lenient::text")]
            confirmation_status: String,
            #[serde(default)]
            err: Option<Value>,
        }
        #[derive(Deserialize)]
        struct Wrapped {
            value: Vec<Option<One>>,
        }
        let answer: Wrapped = self
            .call("getSignatureStatuses", json!([[signature.to_base58()], { "searchTransactionHistory": true }]))
            .await?;
        Ok(answer.value.into_iter().next().flatten().map(|one| Status {
            confirmation: one.confirmation_status,
            error: one.err.filter(|err| !err.is_null()).map(|err| err.to_string()),
        }))
    }

    /// The current block height, to tell when a blockhash has expired.
    pub async fn block_height(&self) -> Result<u64, RpcError> {
        self.call("getBlockHeight", json!([{ "commitment": "confirmed" }])).await
    }
}

/// The mint and amount of an SPL token account: mint at bytes 0..32, amount (u64, little-endian) at
/// bytes 64..72.
pub fn token_account(data: &[u8]) -> Option<(Pubkey, u64)> {
    let mint: [u8; 32] = data.get(0..32)?.try_into().ok()?;
    let amount: [u8; 8] = data.get(64..72)?.try_into().ok()?;
    Some((Pubkey::new(mint), u64::from_le_bytes(amount)))
}

/// The decimals of an SPL mint: byte 44.
pub fn mint_decimals(data: &[u8]) -> Option<u8> {
    if data.len() < 82 {
        return None;
    }
    data.get(44).copied()
}

#[cfg(test)]
#[path = "tests/rpc.rs"]
mod tests;
