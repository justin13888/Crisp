# Build context must be the repo root (workspace contains both core/ and server/).
# docker build -f server/Dockerfile .

FROM rust:slim AS builder
WORKDIR /workspace

# Layer-cache dependency downloads separately from source compilation.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY core/Cargo.toml core/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml
RUN mkdir -p core/src server/src \
    && echo "pub fn dummy() {}" > core/src/lib.rs \
    && printf 'fn main() {}\n' > server/src/main.rs \
    && cargo build --release -p crisp-server \
    && rm -rf core/src server/src

COPY core/ core/
COPY server/ server/
RUN touch core/src/lib.rs server/src/main.rs \
    && cargo build --release -p crisp-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/crisp-server /usr/local/bin/crisp-server
EXPOSE 8080
CMD ["crisp-server"]
