# Build context must be the repo root (the workspace spans core/, server/ and wasm/).
#   docker build -f server/Dockerfile .
#
# Build and runtime images share one Debian release (bookworm), so the binary
# links against the same glibc it runs with.

FROM rust:1-slim-bookworm AS builder
WORKDIR /workspace

# The repo's rust-toolchain.toml is deliberately NOT copied: it pins developer
# components (rust-analyzer, llvm-tools) that a release build does not need,
# and rustup would download them on first use. The image's own minimal-profile
# toolchain (rustc, cargo, rust-std) is used instead.

# Layer-cache dependency compilation separately from the sources: build the
# workspace once against empty stub crates, so only a Cargo.toml/Cargo.lock
# change re-downloads and recompiles dependencies.
#
# Cargo must load every workspace member's manifest, so the `wasm` member is
# present too, but only as its manifest and a stub crate root: it is never
# built here (`-p crisp-server`), and leaving out its build.rs keeps the
# spargen client generator, and with it spec/openapi.json, out of the image.
# The stub stays for the real build below, which copies only core/ and server/.
COPY Cargo.toml Cargo.lock ./
COPY core/Cargo.toml core/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml
COPY wasm/Cargo.toml wasm/Cargo.toml
RUN mkdir -p core/src server/src wasm/src \
    && echo "pub fn dummy() {}" > wasm/src/lib.rs \
    && echo "pub fn dummy() {}" > core/src/lib.rs \
    && echo "pub fn dummy() {}" > server/src/lib.rs \
    && printf 'fn main() {}\n' > server/src/main.rs \
    && cargo build --release --locked -p crisp-server \
    && rm -rf core/src server/src

COPY core/ core/
COPY server/ server/
# Touch the crate roots so cargo rebuilds them rather than reusing the stubs.
RUN touch core/src/lib.rs server/src/lib.rs server/src/main.rs \
    && cargo build --release --locked -p crisp-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --uid 10001 --shell /usr/sbin/nologin crisp
COPY --from=builder /workspace/target/release/crisp-server /usr/local/bin/crisp-server
USER crisp
EXPOSE 8080
# The server drains in-flight requests on SIGTERM/SIGINT before exiting.
STOPSIGNAL SIGTERM
CMD ["crisp-server"]
