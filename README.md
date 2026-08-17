<title>WIP</title>

English | [日本語 (Japanese)](./README_JA.md)

# WIP — Project Management Tool

A lightweight, fast project management tool with ticketing, Gantt charts, a wiki, and notifications. The backend is built with Rust (Axum), and the frontend with React (TypeScript).

## Key Features

- **Ticket management** — issues/tickets with status, priority, labels, assignees, and comment threads
- **Gantt chart** — visual scheduling and dependency tracking
- **Wiki** — internal documentation linked to tickets, with revision history
- **Notifications** — in-app notification feed for ticket and wiki events
- **Auth** — JWT-based authentication, TOTP, and WebAuthn (passkeys) via a dedicated `auth-core` crate

## Architecture

The backend (`rust/`) follows Clean Architecture, split into three layers:

```
rust/src/
├── domain/          # Domain models and domain services (business logic)
├── infrastructure/  # Repository implementations (PostgreSQL via sqlx), mail sending, etc.
├── presentation/     # HTTP handlers, middleware
├── routes.rs         # Route definitions
├── config.rs          # Configuration loaded from environment variables
└── main.rs            # Entry point
```

Authentication is factored out into its own crate at `rust/auth-core/`, handling JWT issuance/verification, TOTP, and WebAuthn (passkeys).

The frontend (`frontend/`) is built with Vite + React + TypeScript. The API client is auto-generated from the OpenAPI schema using [orval](https://orval.dev/) (`npm run generate:api`).

## Getting Started

### Prerequisites

- Rust 1.90+
- Node.js 20+
- PostgreSQL 16

### Quick Start

```bash
createdb wip
cp .env.example .env  # edit DATABASE_URL etc. to match your environment

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

```bash
cd rust
cargo test --workspace
```

## License

[MIT](./LICENSE)
