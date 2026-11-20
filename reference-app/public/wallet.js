// Solana wallet bridge: finds wallets through the Wallet Standard, connects one, and asks it to
// sign a message or sign and send a transaction. The app builds the messages and checks the
// signatures.
//
// Each wallet call times out after 120 seconds, because a popup closed without an answer can leave
// its promise pending forever.
(() => {
  "use strict";

  const TIMEOUT_MS = 120_000;
  const wallets = new Map();
  let current = null; // { wallet, account, name } or { provider, address, name }

  const api = {
    register(...found) {
      for (const wallet of found) {
        const features = wallet.features || {};
        const solana = (wallet.chains || []).some((chain) => chain.startsWith("solana:"));
        if (solana && features["standard:connect"] && features["solana:signMessage"]) {
          wallets.set(wallet.name, wallet);
        }
      }
      return () => {};
    },
  };

  // Wallets loaded before this script respond to app-ready; wallets loaded after it dispatch register-wallet.
  window.addEventListener("wallet-standard:register-wallet", (event) => {
    try {
      event.detail(api);
    } catch (_) {}
  });
  window.dispatchEvent(new CustomEvent("wallet-standard:app-ready", { detail: api }));

  function timed(promise, what) {
    return Promise.race([
      promise,
      new Promise((_, reject) => setTimeout(() => reject(new Error(what + " got no answer from the wallet")), TIMEOUT_MS)),
    ]);
  }

  function bytes(value) {
    return value instanceof Uint8Array ? value : new Uint8Array(value);
  }

  // Fallback for wallets that only inject a provider (window.phantom.solana, window.solflare,
  // window.solana). Supports connect and signMessage only.
  function legacy() {
    return (window.phantom && window.phantom.solana) || window.solflare || window.solana || null;
  }

  window.pascalWallet = {
    // Names of the Wallet Standard wallets, or "Injected wallet" when only a legacy provider exists.
    list() {
      const names = Array.from(wallets.keys());
      if (legacy() && names.length === 0) names.push("Injected wallet");
      return names;
    },

    // Connects `name` (or the first wallet found) and returns its base58 address and name.
    async connect(name) {
      const wallet = (name && wallets.get(name)) || wallets.values().next().value;
      if (wallet) {
        const answer = await timed(wallet.features["standard:connect"].connect(), "Connecting");
        const account = (answer.accounts || wallet.accounts || [])[0];
        if (!account) throw new Error("The wallet gave no account");
        current = { wallet, account, name: wallet.name };
        return { address: account.address, name: wallet.name };
      }
      const provider = legacy();
      if (!provider) throw new Error("No Solana wallet was found in this browser");
      const answer = await timed(provider.connect(), "Connecting");
      const address = (answer && answer.publicKey ? answer.publicKey : provider.publicKey).toString();
      current = { provider, address, name: "Injected wallet" };
      return { address, name: "Injected wallet" };
    },

    async disconnect() {
      const held = current;
      current = null;
      try {
        if (held && held.wallet && held.wallet.features["standard:disconnect"]) {
          await held.wallet.features["standard:disconnect"].disconnect();
        } else if (held && held.provider && held.provider.disconnect) {
          await held.provider.disconnect();
        }
      } catch (_) {}
    },

    connected() {
      return current ? (current.account ? current.account.address : current.address) : null;
    },

    // Signs `message` (bytes) as given and returns the 64-byte signature.
    async signMessage(message) {
      if (!current) throw new Error("No wallet is connected");
      const data = bytes(message);
      if (current.wallet) {
        const feature = current.wallet.features["solana:signMessage"];
        const [output] = await timed(feature.signMessage({ account: current.account, message: data }), "Signing");
        return bytes(output.signature);
      }
      const answer = await timed(current.provider.signMessage(data, "utf8"), "Signing");
      return bytes(answer.signature || answer);
    },

    // Signs and sends a serialized transaction and returns its signature bytes.
    async signAndSendTransaction(transaction) {
      if (!current || !current.wallet) throw new Error("Deposits need a wallet that supports the Wallet Standard");
      const feature = current.wallet.features["solana:signAndSendTransaction"];
      if (!feature) throw new Error(current.name + " cannot send transactions from this page");
      const [output] = await timed(
        feature.signAndSendTransaction({
          account: current.account,
          chain: "solana:mainnet",
          transaction: bytes(transaction),
          options: { preflightCommitment: "confirmed" },
        }),
        "Sending",
      );
      return bytes(output.signature);
    },
  };
})();
