# Contributing to Senn

Thank you for your interest in contributing. This document outlines how to report issues and submit changes.

## Before You Start

- **Do not include secrets**, API keys, tokens, connection strings, or confidential data in issues or pull requests
- **Do not paste internal ticket numbers** or private infrastructure names unrelated to this OSS project
- Review [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) and [SECURITY.md](SECURITY.md)

## Reporting Issues

1. Search existing issues to avoid duplicates
2. Use a clear title and describe expected vs actual behavior
3. Include reproduction steps and environment (OS, Rust, Node.js)

## Security Reports

Do **not** file public issues for vulnerabilities. Follow [SECURITY.md](SECURITY.md).

## Submitting Pull Requests

1. Fork (or clone) and create a feature branch
2. Make changes and run the local checks below
3. Keep commits focused; write a clear commit message
4. Open a PR with a short summary of intent and test evidence

## Local Development

### Rust

```bash
cd rust
export SQLX_OFFLINE=true
cargo check --workspace
cargo test --workspace
```

For a normal local run against PostgreSQL, set `DATABASE_URL` in `.env` (see `.env.example`) and use `cargo run` from `rust/`.

### Frontend

```bash
cd frontend
npm ci
npm run build
npm test
npm audit
```

### Full local CI (recommended before PR)

From a machine that has the OSSP helper scripts (maintainers), or equivalently:

```bash
# Rust
cd rust && SQLX_OFFLINE=true cargo check --workspace && SQLX_OFFLINE=true cargo test --workspace

# Frontend
cd frontend && npm ci && npm run build && npm test && npm audit
```

`npm audit` should report **0 high** and **0 critical**.

## Code Guidelines

- Rust: `rustfmt` / `clippy` conventions
- Frontend: project ESLint / Prettier settings
- Prefer small, reviewable commits

## Questions?

Open a normal (non-security) issue, or see [README.md](README.md) / [README_JA.md](README_JA.md).
