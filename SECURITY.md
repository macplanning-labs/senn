# Security Policy

## Reporting Vulnerabilities

If you discover a security vulnerability, **do not** create a public issue.

1. Email **`y.yoshikawa@macplanning.com`** with the details below (preferred), **or**
2. Open a **private** security advisory on the project's GitHub repository  
   (Repository → Security → Advisories → New draft security advisory), if available.

Wait for acknowledgment before any public disclosure.

Include:

- Description and impact
- Reproduction steps (PoC without exploiting third parties)
- Affected versions / commit if known

Please allow reasonable time for a fix before public disclosure.

## Supported Versions

| Version | Supported |
| --- | --- |
| `main` (latest) | Yes — bug fixes and security patches |
| Latest release tag | Yes — patch-level updates when feasible |
| Older tags | Best-effort / community PRs welcome |

## Security Updates

- Prefer staying on `main` or the latest release tag
- Run `cargo update` / `npm audit` regularly in your deployment pipeline
- Local gate for contributors: `npm audit` with **0 high / 0 critical**, and Rust checks with `SQLX_OFFLINE=true`

## Development Practices

- Review before merge to `main`
- Dependency audits (`cargo audit` / `npm audit`) as part of release hygiene
- Prefer local CI green (`cargo check|test --workspace`, `npm run build`, `npm test`) before release
