English | [日本語 (Japanese)](./README_JA.md)

# SENN — Project Management Tool

> **Public Beta** — SENN is usable, but bugs and missing features remain, and behavior may change between updates. **Back up your data before updating** (a database upgraded to a newer version cannot be used with an older version again). Older releases stay available as [tags](https://github.com/macplanning-labs/senn/tags) — for example, `v0.1.1` is the last release before the access-control redesign; run it with `git checkout v0.1.1`, or set `SENN_IMAGE_TAG=sha-6ebc8da` in `.env` to use its pre-built images.

A lightweight, fast project management tool with ticketing, Gantt charts, a wiki, and notifications. The backend is built with Rust (Axum), and the frontend with React (TypeScript).

## Getting Started (OSS / Self-host)

Run SENN on your own machine or server. Your data stays in your Docker volumes.

### Requirements
- Docker / Docker Compose
- Port `8151` free (UI)

### Start

Run these five lines in a terminal (macOS / Linux, or WSL2 on Windows):

```bash
git clone https://github.com/macplanning-labs/senn.git
cd senn
cp .env.example .env
sed -i.bak "s|^DB_PASSWORD=.*|DB_PASSWORD=$(openssl rand -hex 24)|; s|^JWT_SECRET_KEY=.*|JWT_SECRET_KEY=$(openssl rand -base64 48 | tr -d '\n')|" .env && rm .env.bak
docker compose up
```

- Line 3 creates your config file (`.env`).
- Line 4 fills in `DB_PASSWORD` and `JWT_SECRET_KEY` with random values. Both are left **empty** in `.env.example` on purpose, and `docker compose` stops right away with a clear message if either is empty.
- Line 5 starts everything. **The first run takes a few minutes to download images** (depending on your connection). Keep this terminal open — closing it stops SENN.

No `sed` / `openssl` (e.g. plain Windows)? Open `.env` in an editor and set `DB_PASSWORD=` to letters and digits only (it is embedded in a URL) and `JWT_SECRET_KEY=` to a random string of 32+ characters, then run `docker compose up`.

When you see `listening on 0.0.0.0:8151`, open **http://localhost:8151**

### First use
1. Click **Sign up** and enter username / email / password (8+ characters). You are signed in automatically.
2. Create a **Team**: click the **+** next to **Teams** in the sidebar (or **Create your first team**). Teams are required before tickets.
3. Click **Create a ticket** and enter a title. Your new team is already selected.

### Adding people
Only the **first** account can sign up freely (that person sets up the server). Everyone after that joins **by invitation**:

1. Open the team's member settings and use **Invite**: enter the person's email and click **Send invitation**.
2. If email is not configured (the default), SENN shows a link instead. Send that link to the person yourself — it works once and expires after 7 days.
3. The person opens the link, chooses a username and password, and joins the team.

To change this, set these in `.env` and restart:
- `SENN_INTERNAL_EMAIL_DOMAINS=example.com` — people with these email domains can sign up themselves after verifying their address by email. **Configure email (`EMAIL_*` in `.env`) first**: without email, the verification link is shown on the sign-up screen, so anyone could claim an address in that domain.
- `SENN_REGISTRATION_MODE=open` — anyone who can reach the server can sign up. Use this only on a private network.

### Stop, restart, reset
- **Stop**: press `Ctrl+C` **once** in the terminal, then `docker compose down`. Your data is kept.
- **Start again**: `docker compose up` (no `--build` needed).
- **Wipe everything** (irreversible): `docker compose down -v`

### For those who want to build from source

To build the Docker images yourself instead of downloading pre-built ones:

```bash
docker compose up --build
```

This builds both backend and frontend images from the source code in this repository. Building takes 10–15 minutes the first time, depending on your CPU and network. The source code is publicly available on GitHub.

### When downloads are slow

By default, `docker compose up` downloads pre-built images from `ghcr.io/macplanning-labs`. If downloads are slow or unavailable, you have two options:

1. **Build from source**: Use `docker compose up --build` instead.
2. **Use a different registry**: Add `SENN_IMAGE_REGISTRY` to `.env`, pointing to your organization's registry or a mirror. For example:
   ```text
   SENN_IMAGE_REGISTRY=registry.example.com/macplanning-labs
   ```
   This changes where `docker compose` downloads the images from.

Advanced (Japanese): [`docs/利用ガイド_OSS自己構築.md`](./docs/利用ガイド_OSS自己構築.md)

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
# set DB_PASSWORD and JWT_SECRET_KEY (see Start above), then:
docker compose up
```

`DB_PASSWORD` and `JWT_SECRET_KEY` must be set in `.env` before the first run — the one-line `sed` in [Start](#start) does it for you.

Open http://localhost:8151 in your browser. Database tables are created automatically on first start (`RUST_RUN_MIGRATIONS=true`). No seed users are bundled — create your first account from the in-app registration screen at http://localhost:8151/register, then log in at http://localhost:8151/login. Further accounts join by invitation (see [Adding people](#adding-people)).

To reset the database and start clean:

```bash
docker compose down -v
docker compose up
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
