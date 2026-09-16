# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Community documents: `NOTICE`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`
- Admin / settings / chat integration / notification preferences synced into the public tree
- `google-auth` crate (Google Workspace service-account JWT helper)

### Changed

- Frontend dependency updates to clear npm audit High/Critical/Moderate findings
- OSS branding and sanitization (demo ticket keys, no `drive-core` in public workspace)

### Fixed

- Internal-only references removed from the extract tree for public gates
- Community docs aligned with `LICENSE` copyright and concrete security reporting path

### Known limitations

- Deployment / cloud integration docs are still minimal
- First SemVer tag (`v0.1.0`) is not cut yet (pending maintainer release approval)
