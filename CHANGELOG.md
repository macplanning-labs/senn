# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-09-16

First public OSS release of Senn.

### Added

- Community documents: `NOTICE`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`
- Admin / settings / chat integration / notification preferences in the public tree
- `google-auth` crate (Google Workspace service-account JWT helper)

### Changed

- Frontend dependency updates clearing npm audit High/Critical/Moderate findings
- OSS branding and sanitization (demo ticket keys; no `drive-core` in the public workspace)
- Local setup examples use database name `senn`
- CONTRIBUTING local CI aligned with `OSSP/scripts/run_local_ci.sh` (`SQLX_OFFLINE=true`, `cargo check|test --workspace`, `npm ci|build|test`)

### Fixed

- Internal-only references removed from the extract tree for public gates
- NOTICE copyright set to MacPlanning Co., Ltd. (2024); `LICENSE` matched
- Security contact documented (`y.yoshikawa@macplanning.com`)

### Known limitations

- Deployment / cloud integration documentation is still minimal
- Cycle 4+ (e.g. SES-related) work is out of scope for this extract
