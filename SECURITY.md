# Security Policy

## Reporting Vulnerabilities

If you discover a security vulnerability, please **do not** create a public GitHub issue. Instead:

1. Email security details to the maintainers (refer to [CONTRIBUTING.md](CONTRIBUTING.md) for contact information)
2. Include steps to reproduce and impact assessment
3. Allow time for a response and patch before public disclosure

## Supported Versions

- **Main branch** (`main`): Receives bug fixes and security patches
- **Latest release tag**: Generally supported for patch-level updates
- Older versions: Community contributions are welcome but not actively maintained

## Security Updates

Security patches are released as needed. Please keep your installation up to date by:

- Monitoring GitHub releases
- Running `cargo update` (Rust) and `npm update` (frontend) regularly
- Reviewing dependency audits

## Development Practices

- Mandatory code review before merge to main
- Regular dependency audits via `cargo audit` and `npm audit`
- Continuous integration checks on all pull requests
