# auth-core

WIP（`amagi019/QA_Tool`）・Sophia、および将来の他Rustサービス共通の認証ライブラリ。

詳細な方針・設計背景は Claude Project「認証・共通基盤統合」内のドキュメント
「認証ロジックのライブラリ化（クレート化）およびWIP仕様への一元統一」を参照。

## Step 1（このクレートの新設）で行ったこと

- WIPの既存実装（`jwt_service.rs` / `totp_service.rs` / `webauthn_service.rs` /
  `middleware/rate_limiter.rs`、`auth_service.rs`のパスワード/TOTP部分）を
  `domain::jwt` / `domain::totp` / `domain::webauthn` / `domain::password` /
  `infrastructure::rate_limit` に移植（既存の単体テストも合わせて移植）。
- 方針ドキュメント1.2/1.2.1節の設計に基づき、アカウント単位の試行回数ロック
  ミドルウェア（`infrastructure::rate_limit::attempt_lock_middleware`）を新規実装。
- 方針ドキュメント5〜7章の設計に基づき、`domain::password_policy` /
  `domain::one_time_token` / `domain::mfa_policy` / `domain::audit` /
  `presentation::extractors` / `presentation::middleware` をスケルトンとして新規追加。
- `rust/Cargo.toml` を「ルートパッケージ（`wip`）を維持したままのワークスペース」
  に変更し、このクレートを `[workspace] members = ["auth-core"]` として追加。
  既存の `wip` パッケージのディレクトリ構成・Dockerfile・CI（`ci.yml`）は
  変更していない（`cargo build` / `cargo test` はワークスペース全体を対象にする
  ため、`ci.yml` の既存ジョブがそのままこのクレートも検証する）。

## Step 1時点で見つかった、方針ドキュメントとの相違点（要確認）

1. **WIPは現在デュアル認証構成**: 方針ドキュメント1章の比較表は
   WIP=「JWT（アクセス30分/リフレッシュ7日）+ ブラックリスト」のみとしているが、
   実際のWIPはWeb UI向けにセッションCookie認証（`tower-sessions`、
   `middleware/auth.rs`の`require_auth`）も別途持っており、JWT Bearer認証
   （`middleware/jwt_auth.rs`）はAPIルート専用。Step 2でauth-core適用範囲を
   決める際にこの二重構成をどう扱うか（Web UI側もJWTに寄せるのか、
   セッション認証は当面残すのか）の確認が必要。
2. **JWTクレーム形状がDjango依存**: WIPのJWTはDjango
   （`rest_framework_simplejwt`）と同一の`SECRET_KEY`で相互検証できるよう、
   クレーム形状（`token_type`/`jti`/`user_id`を文字列化 等）をDjango仕様に
   固定している。方針ドキュメント7章の汎用`Claims{sub,roles,exp,iss,extra}`とは
   非互換のため、`domain::jwt`では両方を実装し、既存の互換層を`django_compat`
   モジュールとして分離した。Django側との相互運用がいつまで必要か
   （Djangoを段階的に廃止するのか）によって、Step 2/3での統合方針が変わる。
3. **Passkeyログイン検証は実装済みだった**: ロードマップは「WIPの未実装である
   Passkeyログイン検証を追加実装する」としているが、実際には
   `webauthn_service.rs`に discoverable credential 方式のログイン検証
   （`start_authentication`/`identify_authentication`/`finish_authentication`）が
   既に実装済みだった。そのまま`domain::webauthn`に移植したので、Step 1時点で
   追加実装は不要だった。
4. **`security-gate.yml`は既に全体を対象に組み込み済み**: リポジトリの
   `.github/workflows/security.yml`は`amagi019/mac-planning-standards`の
   再利用ワークフローを`working_directory: "."`（リポジトリ全体）で呼んでおり、
   `auth-core`もそのまま対象になる。同様に`ci.yml`のRustジョブも
   `cargo build --verbose` / `cargo test --verbose`をワークスペース全体に対して
   実行するため、`auth-core`が追加された時点で自動的にCI対象へ入る。
   新規ワークフローファイルの追加は不要だった。
5. **TOTPの秘密鍵保存方式が2系統ある**: `totp_service.rs`のAES-GCM暗号化
   保存（新規Rust実装向け）と、`auth_service.rs`のBase32平文検証
   （DjangoのTotpDeviceテーブルと共有、pyotp互換）が並存していた。
   `domain::totp`では両方を維持し（`encrypt_secret`/`decrypt_secret`系と
   `verify_code_base32`系）、WIPが実運用でどちらを使い続けるかはStep 2で確認する。
6. **`domain::one_time_token`はSophia実コード未参照のスケルトン**: この
   セッションではSophiaリポジトリのパートナー認証トークン実装を直接
   参照できなかったため、方針ドキュメント5章の記述からトレイト設計のみ
   起こしてある。Step 3で実装を突き合わせること。

## 未着手（Step 2以降）

- WIPの21ファイル（`presentation/handlers/*_api.rs`等）が参照している
  `jwt_service` / `totp_service` / `webauthn_service` / `middleware::jwt_auth` /
  `middleware::rate_limiter`の呼び出し箇所を`auth-core`経由に差し替える作業
  （ロードマップ上も明示的にStep 2の範囲）。
- `jwt_blacklist_repo.rs`（sqlx実装）に`domain::jwt::TokenBlacklist`トレイトを
  実装させる配線。
- Sophia用の`LegacyHashVerifier`（bcrypt）実装。
