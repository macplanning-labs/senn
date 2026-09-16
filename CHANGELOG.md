# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial OSS extraction of Senn project
- Community documents: CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md
- Admin and settings cycle sync to OSS-extract repository

### Changed
- README and README_JA updated for OSS context
- Dependency audit and vulnerability fixes for frontend

### Fixed
- npm audit High/Critical vulnerabilities resolved
- Internal references and sensitive data sanitized from public tree

## [0.1.0] - Initial OSS Release (Forthcoming)

This is the initial release of Senn as an open-source project, derived from the MacPlanning internal codebase.

### Features
- Rust backend with async/await and modern web patterns
- React-based frontend SPA with TypeScript
- Admin and settings interfaces
- Authentication and role-based access control

### Known Limitations
- Deployment and cloud integration documentation is minimal
- Cycle 4+ features (e.g., SES services) may require additional setup
