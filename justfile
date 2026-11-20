check: fmt clippy wasm test features doc app

fmt:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Lint the browser transport for wasm32.
wasm:
    cargo clippy -p df-pascal-core --no-default-features --features sign,web --target wasm32-unknown-unknown -- -D warnings

# Unit tests: signing vectors from Pascal's docs and Python quickstart, the request envelope, the
# client against a fake server, ticks, fees, Solana addresses and transactions, websocket frames.
test:
    cargo test --workspace

# Read-only live tests against Pascal's production read API and market data websocket. Run
# manually.
live:
    cargo test -p df-pascal-core --test live -- --ignored --test-threads 1

# Check that builds without the `sign` feature pass clippy and do not depend on ed25519-dalek.
features:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo clippy -p df-pascal-core --no-default-features -- -D warnings
    cargo clippy -p df-pascal-core --no-default-features --features web --target wasm32-unknown-unknown -- -D warnings
    if cargo tree -p df-pascal-core --no-default-features -e no-dev --target all | grep -q ' ed25519-dalek v'; then
        echo "df-pascal-core pulls in ed25519-dalek without sign" >&2
        exit 1
    fi

doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# Format check and lint of the reference app (wasm32).
app:
    cd reference-app && cargo fmt --check && cargo clippy --target wasm32-unknown-unknown -- -D warnings

# Run the reference app at http://localhost:5173.
serve:
    cd reference-app && trunk serve --port 5173

# Build the reference app and deploy it to Cloudflare Pages (project df-pascal-web).
deploy-app:
    cd reference-app && rm -rf dist && trunk build --release && wrangler pages deploy dist --project-name=df-pascal-web --branch main
