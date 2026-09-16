# Contributing to Senn

Thank you for your interest in contributing! This document outlines the process for reporting issues and submitting pull requests.

## Before You Start

- **Do not include secrets, API keys, tokens, or confidential information** in issues or pull requests
- **Do not share internal ticket numbers or references** unrelated to the OSS project
- Review the [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) and [SECURITY.md](SECURITY.md)

## Reporting Issues

1. Check existing issues to avoid duplicates
2. Provide a clear title and detailed description
3. Include steps to reproduce and expected behavior
4. Describe your environment (OS, Rust version, Node.js version, etc.)

## Submitting Pull Requests

1. Fork the repository and create a feature branch
2. Make your changes and test locally
3. Ensure all tests pass and builds succeed
4. Write a clear commit message
5. Submit your PR with a description of what it addresses

## Local Development

### Rust Setup

```bash
cd rust
cargo test
cargo build --release
```

### Frontend Setup

```bash
cd frontend
npm install
npm run build
npm run test
```

### Full CI Check

From the project root, run:

```bash
# Rust tests
cd rust && cargo test --workspace

# Frontend audits and tests
cd frontend && npm audit && npm run build && npm run test
```

## Code Guidelines

- Follow Rust conventions (see `rustfmt`, `clippy`)
- Follow JavaScript/TypeScript conventions (see `.prettierrc`, `.eslintrc`)
- Write meaningful commit messages
- Keep commits logically organized

## Questions?

Open an issue or check the README for more information.
