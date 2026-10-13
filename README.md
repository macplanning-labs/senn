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

## Ask AI to work on tickets (Claude Pro / Max)

From a ticket, click **Ask AI**, and Claude Code works on it in an isolated container on **your own machine**, using **your own Claude Pro / Max subscription**. The result is a commit (and, if you allow it, a pushed branch) for a human to review.

**Before you start**
- **Data is sent to Anthropic.** When AI works on a ticket, the ticket's content (description, comments, author names, links) and the code of the target repository are sent to Anthropic through the Claude account configured on the runner. SENN asks you to confirm this when you enable the feature for a project and each time you run it.
- **It uses your subscription**, via a token created with `claude setup-token`. Pay-as-you-go API keys are not used (the runner refuses to start if `ANTHROPIC_API_KEY` is set).
- **Always review AI changes before merging.** Instructions hidden in a ticket can steer the AI (prompt injection); this cannot be fully prevented.

**How it stays contained**
- The runner polls SENN (nothing connects in), checks the signed request, and runs Claude Code in a disposable container with a read-only root, no capabilities, and only the working directory mounted.
- **Your real Claude token never enters the AI's container.** A per-run gateway holds it, sends requests only to `api.anthropic.com`, and refuses server-side tools (web fetch/search, MCP, code execution) that could carry data elsewhere.
- AI may only work on repositories a SENN system administrator allows, which must also be on the runner's own allowlist. By default nothing is pushed.

**Set up**: see [`tools/ai-runner/README.md`](./tools/ai-runner/README.md) (Japanese). In short:
1. As the system administrator (the first account on a new server), open **Admin → AI auto-run**: save limits, allow repositories, and add a runner (copy its token and signing secret).
2. On the runner machine: `docker build` the sandbox image, run `docker/setup-networks.sh`, then `./verify-sandbox.sh`.
3. Create your token with `claude setup-token`, set the environment variables, and start `node src/runner.mjs`.
4. In the project settings, enable **AI auto-run**, then click **Ask AI** on a ticket.

## Use SENN from your AI (MCP)

`mcp-server/` is an MCP server that lets your own AI client (Claude Code, Claude Desktop, Cursor, …) search, create and update tickets, read and write the wiki, and record AI work **on your own SENN server**. It also ships `senn-ai-run`, a small CLI to report the start and end of AI work.

**Before you start**
- **Tool results are sent to your AI provider.** Ticket descriptions, comments and wiki pages returned by the tools go to the AI you use (Anthropic, for Claude). Use it only with projects whose content may be shown to that AI.
- **Per-person keys.** The AI can only see and do what the key's owner can see and do in SENN.
- **File uploads are restricted.** The wiki-attachment tools read local files. To stop a prompt-injected AI from uploading secrets, only regular files under the current working directory (or `SENN_UPLOAD_ROOTS`) can be sent; dot-files and dot-directories (`.ssh`, `.env`, …), keys and credential files are refused.

**Set up** (details: [`mcp-server/README.md`](./mcp-server/README.md), Japanese):
1. `cd mcp-server && npm ci && npm run build`
2. Put `SENN_BASE_URL=https://<your SENN host>` in `~/.config/senn-ai-agent/credentials.env` (mode 600).
3. Register `bash /path/to/senn/mcp-server/run-with-credentials.sh` as an MCP server (e.g. `claude mcp add senn -- bash /path/to/senn/mcp-server/run-with-credentials.sh`).
4. Ask the AI to "connect to SENN". Open the link it returns, click **Request approval**, and once a system administrator approves (**Admin → AI settings**), the key is saved on your machine automatically — no copy and paste.

**Asking your AI** — Once connected, just talk to the AI in plain words. Examples:
- "Connect to SENN." (first time only; you get an approval link)
- "Find the open tickets in project ABC that were updated this week."
- "Create a ticket from what we just did. Write it so a non-engineer can follow."
- "Record in SENN that you started / finished this task."
- "Summarize this investigation and save it as a wiki page."

Tips:
- **Name the project** (its prefix, e.g. `ABC`) so the AI does not guess.
- **Ask it to show the draft first** before it creates, updates or deletes anything ("show me the ticket text first").
- **Start with read-only requests** (search, summarize) to check the connection and what the AI can see.
- The AI acts **as the key's owner**. Do not share a key, and approve a connection request only if you started it yourself.
- Everything the AI reads goes to your AI provider (see "Before you start" above).

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
