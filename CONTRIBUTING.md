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

Security contact: `y.yoshikawa@macplanning.com`

## Submitting Pull Requests

1. Fork (or clone) and create a feature branch
2. Make changes and run **Local CI** below (must be green)
3. Keep commits focused; write a clear commit message
4. Open a PR with a short summary of intent and test evidence

## Local CI (required before PR)

This matches the maintainer helper `OSSP/scripts/run_local_ci.sh` (GitHub Actions substitute).

### Rust (`SQLX_OFFLINE=true`)

```bash
cd rust
export SQLX_OFFLINE=true
cargo check --workspace
cargo test --workspace
```

### Frontend

```bash
cd frontend
npm ci
npm run build
npm test
```

### One-shot (from repository root)

```bash
# Rust — same as run_local_ci.sh --rust-only
( cd rust && export SQLX_OFFLINE=true && cargo check --workspace && cargo test --workspace )

# Frontend — same as run_local_ci.sh --frontend-only
( cd frontend && npm ci && npm run build && npm test )
```

Maintainers with the OSSP workspace can instead run:

```bash
export OSS_REPO="/path/to/senn-oss-extract"
/bin/bash /path/to/OSSP/scripts/run_local_ci.sh
```

### Dependency audit (recommended)

```bash
cd frontend && npm audit
```

`npm audit` should report **0 high** and **0 critical** before release.

## Local app run (optional)

For a normal run against PostgreSQL, copy `.env.example` → `.env`, create DB `senn` (see README), set `DATABASE_URL`, then:

```bash
cd rust && cargo run
cd frontend && npm run dev
```

## Code Guidelines

- Rust: `rustfmt` / `clippy` conventions
- Frontend: project ESLint / Prettier settings
- Prefer small, reviewable commits

## Questions?

Open a normal (non-security) issue, or see [README.md](README.md) / [README_JA.md](README_JA.md).
