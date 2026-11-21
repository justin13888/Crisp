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

# Needs cargo-mutants (`cargo install cargo-mutants`). Covers committed,
# uncommitted, and untracked (not git-ignored) changes; results land in
# mutants.out/ and a missed mutant fails.
# Mutation-test the Rust lines changed since the merge base with `base`
mutants base="origin/master" *args="":
    #!/usr/bin/env bash
    set -euo pipefail
    diff=$(mktemp)
    trap 'rm -f "$diff"' EXIT
    git diff "$(git merge-base {{base}} HEAD)" -- '*.rs' > "$diff"
    # `git diff` omits untracked files; add each new one as a whole-file diff.
    # `git diff --no-index` exits 1 when the files differ, which they always do.
    git ls-files --others --exclude-standard -z -- '*.rs' |
        while IFS= read -r -d '' file; do
            git diff --no-index -- /dev/null "$file" >> "$diff" || [ $? -eq 1 ]
        done
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

# Fetch the ingredient-index sources pinned in tools/ingredients/rules/rules.toml
# into target/ingredients-cache/ (the importer verifies their SHA-256).
ingredients-fetch:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target/ingredients-cache
    cargo run -q -p crisp-ingredients -- sources | while read -r url file; do
        dest="target/ingredients-cache/$file"
        if [ ! -f "$dest" ]; then
            # Download beside the target and move it into place only when
            # complete, so an interrupted fetch never leaves a partial file.
            curl -sSfL --retry 3 -o "$dest.part" "$url"
            mv "$dest.part" "$dest"
        fi
    done

# Regenerate the ingredient index in spec/ingredients/ (docs/ingredient-index.md)
ingredients: ingredients-fetch
    cargo run -q --release -p crisp-ingredients -- generate

# Fail if spec/ingredients/ is not what the pinned sources and rules produce
ingredients-check: ingredients-fetch
    cargo run -q --release -p crisp-ingredients -- check
