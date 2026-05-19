# Crisp

A lightning-fast, zero-configuration cookbook for recording and sharing recipes. 

Built for cooks, not database administrators. Crisp is a minimal, offline-first recipe manager that cuts out the bloat of traditional productivity apps. Record your favorite meals and share them instantly, with an interface that loads as fast as you can cook.

## 🚀 The Philosophy
- **Zero Config:** No templates, no workspaces, no databases to link.
- **Offline First:** Fully functional without an internet connection.
- **Lightning Fast:** Instant load times with zero UI lag.
- **Fully Open:** An open platform for anyone to record and share. 

## 🏗️ Architecture & Tech Stack
Crisp is designed to be as lightweight as possible, offloading complexity where it makes sense.

* **MVP Platform:** Desktop-first (Cross-platform mobile planned).
* **Data Storage:** Recipes are stored as standardized JSON files.
* **Backend:** A lightweight server stores the JSON files, with metadata indexed in PostgreSQL for blazing-fast search and retrieval.
* **Authentication:** Handled entirely via OpenID Connect (OIDC).

## 📦 Repository Layout

Crisp is a monorepo with two packages, each with its own toolchain:

| Package        | Path      | Stack                  | Role                                            |
| -------------- | --------- | ---------------------- | ----------------------------------------------- |
| Desktop client | `client/` | TypeScript (Bun)       | Offline-first recipe UI                         |
| Server         | `server/` | Rust                   | Stores recipe bundles, indexes metadata for search |

The recipe bundle format is specified in [`spec/`](spec/).

## 🧰 Prerequisites

- [Bun](https://bun.sh) — client runtime, bundler, and test runner
- [Rust (rustup)](https://rustup.rs) — server toolchain (`rust-toolchain.toml` pins the components)
- [just](https://github.com/casey/just) — monorepo task runner
- [Docker](https://docs.docker.com/get-docker/) — runs PostgreSQL (and optionally the server)
- [Lefthook](https://github.com/evilmartians/lefthook) — git hooks manager
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) — server code coverage (`cargo install cargo-llvm-cov`)

## 🛠️ Getting Started (Development)

```bash
# Clone the repository
git clone https://github.com/justin13888/crisp.git
cd crisp

# Activate git hooks
lefthook install
```

### Server

```bash
# 1. Set up environment variables
cp server/.env.example server/.env
#    Edit server/.env — at minimum set CRISP_PLATFORM_SEED:
#    openssl rand -hex 32

# 2. Start PostgreSQL + server (one command)
just dev
#    Equivalent to: docker compose up postgres -d --wait && cargo run -p crisp-server
#    The server runs at http://localhost:8080
```

To run only PostgreSQL (useful when iterating on the server with cargo-watch):

```bash
docker compose up postgres -d
```

To run the fully containerised stack instead:

```bash
docker compose --profile server up
```

### Client

```bash
cd client
bun install
bun run dev
```

## 🔧 Development Commands

**Client (`client/`):**

| Command            | Description          |
| ------------------ | -------------------- |
| `bun install`      | Install dependencies |
| `bun run dev`      | Start with watch     |
| `bun test`         | Run tests            |
| `bun run format`   | Format code          |
| `bun run lint:fix` | Lint and auto-fix    |

**Server (repo root):**

| Command                                  | Description                              |
| ---------------------------------------- | ---------------------------------------- |
| `just dev`                               | Start Postgres + run server (dev)        |
| `docker compose up postgres -d`          | Start only Postgres                      |
| `docker compose --profile server up`     | Run fully containerised stack            |
| `just test`                              | Run tests                                |
| `just format`                            | Format code                              |
| `just lint-fix`                          | Lint and auto-fix                        |
| `just coverage`                          | Run coverage                             |

## 🤖 Git Hooks & CI

This project uses [Lefthook](https://github.com/evilmartians/lefthook). Pre-commit hooks auto-fix formatting and linting on staged files; pre-push hooks run format checks, lint checks, tests, and server coverage.

GitHub Actions runs format checks, linting, tests, and a Rust coverage report on pushes to `master` and pull requests. The coverage report is uploaded as a CI artifact.

## 📄 License

Crisp is licensed under the [GNU AGPL-3.0](LICENSE). Network use of a modified version requires sharing the corresponding source.
