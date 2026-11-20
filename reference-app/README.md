<p align="center">
  <a href="https://dfurrer.com">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://dfu-cl-storybook.pages.dev/brand/markdfu-raised.svg">
      <img src="https://dfu-cl-storybook.pages.dev/brand/markdfu-raised-ink.svg" alt="DFU" width="72">
    </picture>
  </a>
</p>

# df-pascal-web

The reference app for [df-pascal-core](..): a browser client for Pascal written in Rust (Leptos, client side only, compiled to wasm). Every read, stream and signed request goes through the SDK.

Layout: a top bar with the pages and the wallet, a market rail on wide screens, and a status bar with the deployment, clock offset, account and trading key. The market page has the price chart, the book, the order ticket and a lower panel with trades, holders, the event's other markets and the market's terms.

| Page | What it uses |
|---|---|
| Markets | `markets::list`, `resolved`, `text_search`; tags and 24-hour volume |
| Market | `search`, `mark_price_candles`, `recent`, `top_holders`; the `book` and `trades` channels applied to `live::store::Book`; the order ticket (`OrderRequest::outcome`, ticks, fees, collateral); collateral return on mutually exclusive events |
| Portfolio | `account::state` and the `account` channel applied to `live::store::Account`; close a position (reduce-only IOC), cancel, cancel all in a market; PnL history and movers; fills, transfers, resolutions, closed positions and collateral returns, paged by cursor |
| Funds | `deposit_address`, registration with a referral code (wallet), the USDC deposit transaction (`DepositPlan`, `deposit_transaction`, sent by the wallet, followed over JSON-RPC), withdrawals to a wallet's USDC account (wallet) |
| Trading keys | `trading_keys`; a key generated in the page and authorized by the wallet; load an existing key, checked with `is_active`; revoke (wallet) |
| Profile | username and bio, referral codes (generate, claim, apply), ranks, builder totals, taker discount, trader search by X handle |
| Settings | deployment, Solana RPC, the client byte on orders, the clock offset |

## Signing

- Trading requests are signed in the page by the trading key, which the SDK generates and the page keeps in local storage when asked to.
- Wallet requests are prepared by the SDK, shown, signed by the person's Solana wallet through `public/wallet.js` (the Wallet Standard; no libraries), and verified by the SDK before they are sent.
- Deposits are Solana transactions the SDK builds; the wallet signs and sends them.

## Run

Rust 1.94.1 with wasm32 (the repository's `rust-toolchain.toml`), Trunk 0.21. From the repository root:

```bash
just serve
```

Balances and deposits use PublicNode's free Solana RPC by default (Solana's own public endpoint refuses browsers); another RPC can be set in Settings.

## Check

```bash
just app
```

The app is its own Cargo workspace (wasm only, with its own `Cargo.lock`) and depends on the SDK by path.

## Deploy

`dist/` is static, with `_redirects` sending every path to `index.html`. `just deploy-app` builds it and runs `wrangler pages deploy` to the Cloudflare Pages project `df-pascal-web`, live at https://df-pascal-web.pages.dev.
