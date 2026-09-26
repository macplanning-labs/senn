English | [日本語 (Japanese)](./README_JA.md)

# SENN — Project Management Tool

A lightweight, fast project management tool with ticketing, Gantt charts, a wiki, and notifications. The backend is built with Rust (Axum), and the frontend with React (TypeScript).

## Getting Started (OSS / Self-host)

Run SENN on your own machine or server. Your data stays in your Docker volumes.

### Requirements
- Docker / Docker Compose
- Port `8151` free (UI)

### Start

**Recommended** — the startup script creates `.env.oss` with random secrets on first run, so there is nothing to edit:

```bash
git clone https://github.com/macplanning-labs/senn.git
cd senn
./scripts/oss-up.sh
```

**Manual** — use the default compose file yourself. Both `DB_PASSWORD` and `JWT_SECRET_KEY` are left **empty** in `.env.example` on purpose, and `docker compose` stops immediately with an error until you fill them in:

```bash
git clone https://github.com/macplanning-labs/senn.git
cd senn
cp .env.example .env
```

Then set the two values in `.env` (generate them with the commands below) and start:

```bash
openssl rand -hex 24        # → DB_PASSWORD (letters/digits only: it is embedded in a URL)
openssl rand -base64 48     # → JWT_SECRET_KEY (32+ characters)
docker compose up --build
```

Either way, open **http://localhost:8151**

### First use
1. Sign up (username / email / password ≥ 8)
2. Create a **Team** at `/teams` (required before tickets)
3. Create a ticket from the team page

Stop: `./scripts/oss-down.sh` (add `--volumes` to wipe data)

Details: [`docs/利用ガイド_OSS自己構築.md`](./docs/利用ガイド_OSS自己構築.md)

---

## Key Features

- **Ticket management** — issues/tickets with status, priority, labels, assignees, threaded comments (with @mentions), and Linear-style keyboard navigation
- **Teams** — multi-team workspaces with per-team roles, guest access, and team-scoped cycles/boards/gantt/wiki
- **Gantt chart** — visual scheduling and dependency tracking
- **Wiki** — internal documentation linked to tickets, with revision history
- **Notifications** — in-app notification feed for ticket and wiki events
- **Auth** — JWT-based authentication, TOTP, and WebAuthn (passkeys) via a dedicated `auth-core` crate

## Architecture

The UI is a **React SPA**. The backend previously used Askama for some server-rendered screens (dashboard, tickets, gantt, wiki, etc.); those have since been fully replaced by the SPA. The only server-rendered templates left are the pre-login auth shells (`rust/templates/auth/`) — login, MFA, password reset, WebAuthn — which run before the SPA bundle loads.

### Backend (`rust/`)

Clean Architecture in three layers:

```
rust/src/
├── domain/          # Domain models and domain services (business logic)
├── infrastructure/  # Repository implementations (PostgreSQL via sqlx), mail sending, etc.
├── presentation/     # HTTP handlers (JSON API + pre-login auth pages), middleware
├── routes.rs         # Route definitions
├── config.rs          # Configuration loaded from environment variables
└── main.rs            # Entry point
```

`rust/templates/auth/` holds the Askama templates for the pre-login auth shells; everything else is served by the SPA.
Authentication is factored out into `rust/auth-core/` (JWT, TOTP, WebAuthn/passkeys).

### Frontend (`frontend/`)

Vite + React + TypeScript SPA. API client is generated from OpenAPI via [orval](https://orval.dev/) (`npm run generate:api`).

Within the SPA, UI complexity is intentional:

- **Master / settings** (`features/settings/` and similar) — lightweight CRUD forms
- **Tickets / boards / Git activity** — richer interactive UI (kanban, panels, modals)

## Development setup

### Running with Docker

No local Rust / Node / PostgreSQL install needed. Same as [Getting Started](#getting-started-oss--self-host) above:

```bash
cp .env.example .env
docker compose up --build
```

Use `./scripts/oss-up.sh` (secrets are generated for you), or set `DB_PASSWORD` and `JWT_SECRET_KEY` in `.env` before the first run — see [Start](#start).

Open http://localhost:8151 in your browser. Database tables are created automatically on first start (`RUST_RUN_MIGRATIONS=true`). No seed users are bundled — create your first account from the in-app registration screen at http://localhost:8151/register, then log in at http://localhost:8151/login.

To reset the database and start clean:

```bash
docker compose down -v
docker compose up --build
```

### Manual setup (without Docker)

#### Prerequisites

- Rust 1.90+
- Node.js 20+
- PostgreSQL 16

#### Steps

```bash
createdb senn
cp .env.example .env  # edit DATABASE_URL (e.g. postgresql://USER@localhost/senn) etc.

cd rust
cargo run
```

`rust/migrations/20260701000000_initial_schema.sql` is the baseline migration that creates all tables (extracted from a production schema via `pg_dump --schema-only` and cleaned up). Running `cargo run` with a fresh, empty PostgreSQL database and `RUST_RUN_MIGRATIONS=true` (the default in `.env.example`) applies this migration automatically — no manual schema setup needed.

```bash
cd frontend
npm install
npm run dev
```

## Tests

Local CI equivalent (same as [CONTRIBUTING.md](./CONTRIBUTING.md) / maintainer `run_local_ci.sh`):

```bash
cd rust
export SQLX_OFFLINE=true
cargo check --workspace
cargo test --workspace

cd ../frontend
npm ci
npm run build
npm test
```

## License

[MIT](./LICENSE)
