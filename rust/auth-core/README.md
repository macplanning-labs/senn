# auth-core

Authentication library for SENN.

`auth-core` bundles the authentication primitives used by the SENN backend into a standalone crate: JWT issuing and verification, TOTP, WebAuthn (passkeys), password hashing and policy, IP rate limiting, and account-level attempt locking.

## Modules

| Module | Purpose |
| --- | --- |
| `domain::jwt` | Access / refresh token issuing and verification, with a pluggable blacklist and per-audience token policies |
| `domain::totp` | Time-based one-time passwords, including encrypted secret storage helpers |
| `domain::webauthn` | WebAuthn registration and authentication (passkeys) |
| `domain::password` | Argon2 hashing, plus a trait for verifying legacy hashes |
| `domain::password_policy` | Configurable password strength rules |
| `domain::one_time_token` | Single-use tokens (mail links, QR codes) |
| `domain::mfa_policy` | When MFA is required |
| `domain::attempt_lock` | Account-level attempt counting and locking |
| `domain::audit` | Authentication audit event definitions |
| `infrastructure::rate_limit` | IP rate limiting built on `tower_governor`, and the attempt-lock middleware |
| `presentation` | Axum extractors and middleware |

## Design

`auth-core` does not know how users are persisted, and has no domain model of its own for them. Anything that needs storage is delegated to the application through traits — `jwt::TokenBlacklist`, `one_time_token::OneTimeTokenStore`, `attempt_lock::AttemptStore`, `presentation::extractors::AuthUserRepository` and so on. The crate does not depend on a specific database crate such as `sqlx`.

Policy values (token lifetimes, password rules, lock thresholds) are injected by the application rather than hard-coded, so different deployments can keep different policies.

## Usage

This crate is part of the SENN workspace and is not published to crates.io. Within the workspace:

```toml
[dependencies]
auth-core = { path = "../auth-core" }
```

## License

MIT. See [LICENSE](../../LICENSE) in the repository root.
