# Crisp monorepo task runner.
# `just` searches parent directories, so these recipes run from anywhere in the repo.

# --- Development ---

# Start PostgreSQL via Docker Compose, then run the server with env from server/.env.
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    docker compose up postgres -d --wait
    set -a && source server/.env && set +a
    cargo run -p crisp-server

# --- Rust workspace (core + server) ---

# Format all Rust code
format:
    cargo fmt

# Check Rust formatting without modifying files
format-check:
    cargo fmt --check

# Lint the whole workspace
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Lint and auto-fix
lint-fix:
    cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged

# Run all workspace tests
test:
    cargo test --workspace

# Run tests with a coverage report
coverage:
    cargo llvm-cov --workspace --lcov --output-path lcov.info

# Build the `core` crate to WebAssembly for the web client
wasm:
    wasm-pack build core --target web --out-dir ../client/public/crisp-wasm --no-typescript --features wasm
