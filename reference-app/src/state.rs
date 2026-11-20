//! App state shared through context: settings, account, trading key, SDK client and toasts.

use std::rc::Rc;
use std::time::Duration;

use df_pascal_core::client::web::{Web, clock};
use df_pascal_core::client::{ApiError, Credentials, Deployment};
use df_pascal_core::utils::Signer;
use df_pascal_core::{Client, Keypair, Pubkey};
use leptos::prelude::*;

/// The account and the trading key the client signs trading requests with.
#[derive(Clone, Default)]
pub struct Creds {
    pub owner: Option<Pubkey>,
    pub key: Option<Keypair>,
}

impl Credentials for Creds {
    fn owner(&self) -> Option<Pubkey> {
        self.owner
    }

    fn trading_key(&self) -> Option<&dyn Signer> {
        self.key.as_ref().map(|key| key as &dyn Signer)
    }
}

pub type Pascal = Client<Web, Creds>;

#[derive(Clone, Copy)]
pub struct AppState {
    pub deployment: RwSignal<String>,
    pub rpc_url: RwSignal<String>,
    pub metadata: RwSignal<u8>,
    /// The account shown and traded: the connected wallet's, or a watched address.
    pub owner: RwSignal<Option<Pubkey>>,
    /// The connected wallet's name; `None` when the account is only watched.
    pub wallet: RwSignal<Option<String>>,
    /// Public key of the trading key held in this browser.
    pub trading: RwSignal<Option<Pubkey>>,
    /// Incremented after each write so account reads refetch.
    pub revision: RwSignal<u64>,
    pub toasts: RwSignal<Vec<(u64, String, bool)>>,
    key: StoredValue<Option<Keypair>, LocalStorage>,
    client: StoredValue<Rc<Pascal>, LocalStorage>,
}

const DEPLOYMENT: &str = "pascal.deployment";
const RPC: &str = "pascal.rpc";
const METADATA: &str = "pascal.metadata";
const OWNER: &str = "pascal.owner";

fn key_slot(owner: &Pubkey) -> String {
    format!("pascal.trading_key.{owner}")
}

pub fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

pub fn load(name: &str) -> Option<String> {
    storage()?.get_item(name).ok().flatten().filter(|value| !value.is_empty())
}

pub fn save(name: &str, value: Option<&str>) {
    if let Some(storage) = storage() {
        let _ = match value {
            Some(value) => storage.set_item(name, value),
            None => storage.remove_item(name),
        };
    }
}

impl AppState {
    pub fn new() -> AppState {
        let deployment = load(DEPLOYMENT).unwrap_or_else(|| "prod".to_owned());
        // The public mainnet endpoint refuses browsers; a saved copy of it is replaced too.
        let rpc = load(RPC)
            .filter(|url| url != df_pascal_core::utils::solana::rpc::MAINNET)
            .unwrap_or_else(|| df_pascal_core::utils::solana::rpc::PUBLICNODE.to_owned());
        let metadata = load(METADATA).and_then(|text| text.parse().ok()).unwrap_or(0);
        let owner = load(OWNER).and_then(|text| Pubkey::parse(&text).ok());
        let key = owner.and_then(|owner| load(&key_slot(&owner))).and_then(|secret| Keypair::parse(&secret).ok());
        let app = AppState {
            deployment: RwSignal::new(deployment),
            rpc_url: RwSignal::new(rpc),
            metadata: RwSignal::new(metadata),
            owner: RwSignal::new(owner),
            wallet: RwSignal::new(None),
            trading: RwSignal::new(key.as_ref().map(Keypair::pubkey)),
            revision: RwSignal::new(0),
            toasts: RwSignal::new(Vec::new()),
            key: StoredValue::new_local(key),
            client: StoredValue::new_local(Rc::new(Client::new(Web, clock, Creds::default()))),
        };
        app.rebuild();
        app
    }

    pub fn deployment_config(&self) -> Deployment {
        Deployment::named(&self.deployment.get_untracked()).unwrap_or_default()
    }

    /// Replaces the client for the current deployment, account and key, and syncs its clock in the
    /// background.
    fn rebuild(&self) {
        let creds = Creds { owner: self.owner.get_untracked(), key: self.key.get_value() };
        let client = Client::new(Web, clock, creds)
            .with_deployment(self.deployment_config())
            .with_metadata(self.metadata.get_untracked());
        let client = Rc::new(client);
        self.client.set_value(client.clone());
        leptos::task::spawn_local(async move {
            let _ = client.sync_clock().await;
        });
    }

    pub fn client(&self) -> Rc<Pascal> {
        self.client.get_value()
    }

    pub fn has_key(&self) -> bool {
        self.key.with_value(Option::is_some)
    }

    pub fn set_deployment(&self, name: &str) {
        save(DEPLOYMENT, Some(name));
        self.deployment.set(name.to_owned());
        self.rebuild();
        self.bump();
    }

    pub fn set_rpc(&self, url: &str) {
        save(RPC, Some(url));
        self.rpc_url.set(url.to_owned());
    }

    pub fn set_metadata(&self, metadata: u8) {
        save(METADATA, Some(&metadata.to_string()));
        self.metadata.set(metadata);
        self.rebuild();
    }

    /// Switches to `owner`'s account, connected through `wallet` or watched only, and loads its saved
    /// trading key.
    pub fn set_owner(&self, owner: Option<Pubkey>, wallet: Option<String>) {
        save(OWNER, owner.map(|owner| owner.to_base58()).as_deref());
        let key = owner.and_then(|owner| load(&key_slot(&owner))).and_then(|secret| Keypair::parse(&secret).ok());
        self.trading.set(key.as_ref().map(Keypair::pubkey));
        self.key.set_value(key);
        self.owner.set(owner);
        self.wallet.set(wallet);
        self.rebuild();
        self.bump();
    }

    /// Sets the trading key for the current account; `remember` also saves it to localStorage.
    pub fn set_key(&self, key: Option<Keypair>, remember: bool) {
        if let Some(owner) = self.owner.get_untracked() {
            let secret = key.as_ref().filter(|_| remember).map(|key| key.secret_base58().to_string());
            save(&key_slot(&owner), secret.as_deref());
        }
        self.trading.set(key.as_ref().map(Keypair::pubkey));
        self.key.set_value(key);
        self.rebuild();
    }

    pub fn bump(&self) {
        self.revision.update(|revision| *revision += 1);
    }

    pub fn toast(&self, message: impl Into<String>, ok: bool) {
        let id = clock();
        self.toasts.update(|toasts| toasts.push((id, message.into(), ok)));
        let toasts = self.toasts;
        set_timeout(move || toasts.update(|toasts| toasts.retain(|(at, _, _)| *at != id)), Duration::from_secs(8));
    }

    pub fn fail(&self, error: impl std::fmt::Display) {
        self.toast(error.to_string(), false);
    }
}

pub fn use_app() -> AppState {
    use_context::<AppState>().unwrap_or_else(AppState::new)
}

/// Formats an API error, with a warning when a write may have gone through.
pub fn describe(error: &ApiError) -> String {
    let base = match error {
        ApiError::Refused(refusal) => format!("{}: {}", refusal.code, refusal.message),
        other => other.to_string(),
    };
    match error.may_have_landed() {
        true => format!("{base}. It may have gone through: check the account before trying again."),
        false => base,
    }
}

/// The market list, fetched per deployment and shared by the rail, the markets page and the status bar.
#[derive(Clone, Copy)]
pub struct MarketList(pub LocalResource<Result<Vec<df_pascal_core::markets::Market>, String>>);

impl MarketList {
    pub fn provide(app: AppState) {
        let list = LocalResource::new(move || {
            let _ = app.deployment.get();
            let client = app.client();
            async move {
                df_pascal_core::markets::list(&*client).await.map(|reply| reply.data).map_err(|error| describe(&error))
            }
        });
        provide_context(MarketList(list));
    }
}

pub fn use_market_list() -> Option<MarketList> {
    use_context::<MarketList>()
}
