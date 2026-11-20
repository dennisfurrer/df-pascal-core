//! Two-step writes: a [`Prepared`] request holds the body, `auth` and the bytes to sign, and
//! signing it locally or with a wallet's `signMessage` gives a [`Signed`] request to send. With
//! feature `sign`, a signature supplied from outside is verified before it is accepted.

use serde::de::DeserializeOwned;
use serde::{Serialize, Serializer};

use crate::utils::keys::{Pubkey, Signature};
use crate::utils::permit::Auth;
use crate::utils::sign::{SignError, Signer};

/// A request body for one of Pascal's write endpoints.
pub trait Write: Serialize {
    /// Path on the write API.
    const PATH: &'static str;
    /// True if the endpoint takes a JSON array of signed requests and returns one envelope per
    /// item.
    const BATCH: bool = false;
    /// True if the endpoint counts against the order request rate limit.
    const ORDERS: bool = false;
    /// Maximum number of items in one batch.
    const MAX_BATCH: usize = 1;
    /// Type of the `data` field in a successful response.
    type Reply: DeserializeOwned;
}

/// A write ready to sign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared<B> {
    pub body: B,
    pub auth: Auth,
    message: Vec<u8>,
}

impl<B> Prepared<B> {
    #[must_use]
    pub const fn new(body: B, auth: Auth, message: Vec<u8>) -> Prepared<B> {
        Prepared { body, auth, message }
    }

    /// The exact bytes to sign: a binary permit or UTF-8 JSON.
    #[must_use]
    pub fn message(&self) -> &[u8] {
        &self.message
    }

    /// The message as text when it is a JSON permit, for display in a wallet prompt.
    #[must_use]
    pub fn message_text(&self) -> Option<&str> {
        std::str::from_utf8(&self.message).ok().filter(|text| text.starts_with('['))
    }

    /// The key that must sign: `auth.signer`.
    #[must_use]
    pub const fn signer(&self) -> Pubkey {
        self.auth.signer
    }

    /// Signs with `signer`, which must be `auth.signer`.
    pub fn sign(self, signer: &impl Signer) -> Result<Signed<B>, SignError> {
        if signer.pubkey() != self.auth.signer {
            return Err(SignError::Refused(format!(
                "this request is signed by {}, not {}",
                self.auth.signer,
                signer.pubkey()
            )));
        }
        let signature = signer.sign_message(&self.message)?;
        self.with_signature(signature)
    }

    /// Attaches a signature made elsewhere, such as by a wallet or hardware key. With feature
    /// `sign` it must verify under `auth.signer` over [`Prepared::message`].
    pub fn with_signature(self, signature: Signature) -> Result<Signed<B>, SignError> {
        #[cfg(feature = "sign")]
        if !crate::utils::sign::verify(&self.auth.signer, &self.message, &signature) {
            return Err(SignError::Invalid(self.auth.signer));
        }
        Ok(Signed { body: self.body, auth: self.auth, signature })
    }

    /// Attaches a signature without verifying it; Pascal verifies it on receipt. For builds
    /// without `sign`.
    #[must_use]
    pub fn with_signature_unchecked(self, signature: Signature) -> Signed<B> {
        Signed { body: self.body, auth: self.auth, signature }
    }
}

/// A signed write. It serializes as the body's fields followed by `auth`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed<B> {
    pub body: B,
    pub auth: Auth,
    pub signature: Signature,
}

/// `auth` in the form Pascal expects, with numbers as strings.
#[derive(Serialize)]
struct AuthJson {
    client_ts_ms: String,
    recv_window_ms: String,
    owner: Pubkey,
    signer: Pubkey,
    signature: Signature,
}

#[derive(Serialize)]
struct Out<'a, B> {
    #[serde(flatten)]
    body: &'a B,
    auth: AuthJson,
}

impl<B: Serialize> Serialize for Signed<B> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Out {
            body: &self.body,
            auth: AuthJson {
                client_ts_ms: self.auth.client_ts_ms.to_string(),
                recv_window_ms: self.auth.recv_window_ms.to_string(),
                owner: self.auth.owner,
                signer: self.auth.signer,
                signature: self.signature,
            },
        }
        .serialize(serializer)
    }
}

impl<B: Serialize> Signed<B> {
    /// The request as JSON, exactly as sent.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}
