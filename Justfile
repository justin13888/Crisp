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

# --- Rust workspace (core + server + wasm) ---

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

# Check every package file against its line threshold and the shrinking baseline
file-size:
    python3 -B scripts/check-file-size.py

# Test the file-size gate itself
file-size-test:
    python3 -B -m unittest discover -s scripts

# Run tests with a coverage report
coverage:
    cargo llvm-cov --workspace --lcov --output-path lcov.info

# Needs cargo-mutants (`cargo install cargo-mutants`). Covers committed and
# uncommitted changes; results land in mutants.out/ and a missed mutant fails.
# Mutation-test the Rust lines changed since the merge base with `base`
mutants base="origin/master" *args="":
    #!/usr/bin/env bash
    set -euo pipefail
    diff=$(mktemp)
    trap 'rm -f "$diff"' EXIT
    git diff "$(git merge-base {{base}} HEAD)" -- '*.rs' > "$diff"
    if [ ! -s "$diff" ]; then
        echo "No Rust changes since {{base}}: nothing to mutate."
        exit 0
    fi
    cargo mutants --workspace --in-diff "$diff" {{args}}

# Build the client engine (crisp-wasm) and its TypeScript declarations.
# Needs `rustup target add wasm32-unknown-unknown` and wasm-bindgen-cli at the
# version pinned in Cargo.lock (`cargo install wasm-bindgen-cli --version <v>`).
wasm:
    cargo build -p crisp-wasm --target wasm32-unknown-unknown --profile wasm-release
    wasm-bindgen --target web --out-dir client/public/crisp-wasm --out-name crisp \
        target/wasm32-unknown-unknown/wasm-release/crisp_wasm.wasm
    cp client/public/crisp-wasm/crisp.d.ts client/src/generated/crisp.d.ts

# Run the crisp-wasm tests under Node
wasm-test:
    cargo test -p crisp-wasm --target wasm32-unknown-unknown

# Regenerate spec/openapi.json from the server's routes
openapi:
    UPDATE_OPENAPI=1 cargo test -p crisp-server --test openapi

# Regenerate the compliance suite in spec/compliance/
compliance:
    CRISP_UPDATE_COMPLIANCE=1 cargo test -p crisp-core the_published_compliance_suite
