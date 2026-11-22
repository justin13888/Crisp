# Build context must be the repo root (the workspace spans core/, server/ and wasm/).
#   docker build -f server/Dockerfile .
#
# Build and runtime images share one Debian release (bookworm), so the binary
# links against the same glibc it runs with.
#
# Base image policy: both tags are pinned to a Debian release, not to a digest.
# `rust:1-slim-bookworm` tracks the latest stable compiler, as
# rust-toolchain.toml's `stable` channel does for development and CI, and
# `debian:bookworm-slim` receives Debian security updates under the same tag.
# Nothing here bumps a digest automatically, so a digest pin would freeze
# security fixes instead of securing the build. Rebuild with `--pull` on every
# release and at least monthly (docs/deployment.md, "Upgrades").

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
# Cargo must load every workspace member's manifest, so the `wasm`, `mcp`,
# and `tools/ingredients` members are present too, but only as manifests and
# stub crate roots: none is built here (`-p crisp-server`), and leaving out
# the build.rs of wasm and mcp keeps the spargen client generator, and with
# it spec/openapi.json, out of the image. The stubs stay for the real build
# below, which copies only core/, server/, and the ingredient index that
# core/build.rs embeds (spec/ingredients/).
COPY Cargo.toml Cargo.lock ./
COPY core/Cargo.toml core/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml
COPY wasm/Cargo.toml wasm/Cargo.toml
COPY mcp/Cargo.toml mcp/Cargo.toml
COPY tools/ingredients/Cargo.toml tools/ingredients/Cargo.toml
RUN mkdir -p core/src server/src wasm/src mcp/src tools/ingredients/src \
    && echo "pub fn dummy() {}" > wasm/src/lib.rs \
    && echo "pub fn dummy() {}" > mcp/src/lib.rs \
    && printf 'fn main() {}\n' > mcp/src/main.rs \
    && printf 'fn main() {}\n' > tools/ingredients/src/main.rs \
    && echo "pub fn dummy() {}" > core/src/lib.rs \
    && echo "pub fn dummy() {}" > server/src/lib.rs \
    && printf 'fn main() {}\n' > server/src/main.rs \
    && cargo build --release --locked -p crisp-server \
    && rm -rf core/src server/src

COPY spec/ingredients/ spec/ingredients/
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
# A numeric user, so a pod's `runAsNonRoot: true` can verify it.
USER 10001
# The API: plaintext HTTP/1.1 for the Service and ingress in front of the pod.
# The server never terminates TLS, and the image holds no certificate. The
# code default is loopback; inside the container's own network the API binds
# every interface so the Service can reach it.
ENV CRISP_BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
# The Prometheus listener, once CRISP_METRICS_BIND_ADDR=0.0.0.0:9464 binds it
# on the pod network: for an in-cluster scraper, never behind the Ingress.
EXPOSE 9464
# Kubernetes ignores HEALTHCHECK and probes the pod itself (docs/deployment.md);
# this one serves `podman run`/`docker run` while testing an image.
# Liveness: GET /api/v1/health on the loopback port, through bash's /dev/tcp so
# the image needs no HTTP client. It assumes the image's CRISP_BIND_ADDR port.
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/8080 && printf 'GET /api/v1/health HTTP/1.0\\r\\nHost: localhost\\r\\n\\r\\n' >&3 && head -n 1 <&3 | grep -q ' 200 '"]
# The server drains in-flight requests on SIGTERM/SIGINT before exiting.
STOPSIGNAL SIGTERM
CMD ["crisp-server"]
