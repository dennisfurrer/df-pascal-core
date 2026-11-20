//! Calls into `window.pascalWallet` (public/wallet.js); the SDK builds the messages and checks the signatures.

use df_pascal_core::{Pubkey, Signature};
use js_sys::{Array, Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

fn bridge() -> Result<JsValue, String> {
    let window = web_sys::window().ok_or("no window")?;
    let bridge =
        Reflect::get(&window, &JsValue::from_str("pascalWallet")).map_err(|_| "the wallet bridge did not load")?;
    match bridge.is_undefined() {
        true => Err("the wallet bridge did not load".to_owned()),
        false => Ok(bridge),
    }
}

fn said(error: &JsValue) -> String {
    Reflect::get(error, &JsValue::from_str("message"))
        .ok()
        .and_then(|message| message.as_string())
        .or_else(|| error.as_string())
        .unwrap_or_else(|| "the wallet refused".to_owned())
}

async fn call(name: &str, args: &[JsValue]) -> Result<JsValue, String> {
    let bridge = bridge()?;
    let function: Function = Reflect::get(&bridge, &JsValue::from_str(name))
        .map_err(|e| said(&e))?
        .dyn_into()
        .map_err(|_| format!("{name} is missing"))?;
    let args = args.iter().collect::<Array>();
    let answer = function.apply(&bridge, &args).map_err(|e| said(&e))?;
    match answer.dyn_into::<Promise>() {
        Ok(promise) => JsFuture::from(promise).await.map_err(|e| said(&e)),
        Err(value) => Ok(value),
    }
}

/// Names of the wallets found in this browser.
pub fn list() -> Vec<String> {
    let Ok(bridge) = bridge() else { return Vec::new() };
    let Ok(function) = Reflect::get(&bridge, &JsValue::from_str("list")).and_then(|f| f.dyn_into::<Function>()) else {
        return Vec::new();
    };
    function
        .call0(&bridge)
        .ok()
        .and_then(|names| names.dyn_into::<Array>().ok())
        .map(|names| names.iter().filter_map(|name| name.as_string()).collect())
        .unwrap_or_default()
}

/// Connects `name` (or the first wallet found) and returns its address and name.
pub async fn connect(name: Option<String>) -> Result<(Pubkey, String), String> {
    let arg = name.map_or(JsValue::UNDEFINED, |name| JsValue::from_str(&name));
    let answer = call("connect", &[arg]).await?;
    let address =
        Reflect::get(&answer, &JsValue::from_str("address")).ok().and_then(|a| a.as_string()).ok_or("no address")?;
    let name = Reflect::get(&answer, &JsValue::from_str("name")).ok().and_then(|n| n.as_string()).unwrap_or_default();
    Ok((Pubkey::parse(&address).map_err(|e| e.to_string())?, name))
}

pub async fn disconnect() {
    let _ = call("disconnect", &[]).await;
}

/// Asks the wallet to sign `message` as given and returns the signature.
pub async fn sign_message(message: &[u8]) -> Result<Signature, String> {
    let answer = call("signMessage", &[Uint8Array::from(message).into()]).await?;
    Signature::from_slice(&Uint8Array::new(&answer).to_vec()).map_err(|e| e.to_string())
}

/// Asks the wallet to sign and send a serialized transaction and returns its signature.
pub async fn sign_and_send(transaction: &[u8]) -> Result<Signature, String> {
    let answer = call("signAndSendTransaction", &[Uint8Array::from(transaction).into()]).await?;
    Signature::from_slice(&Uint8Array::new(&answer).to_vec()).map_err(|e| e.to_string())
}
