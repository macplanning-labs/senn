/// GitHub API クライアント(App 認証)
///
/// 通信先は固定ホスト(`GITHUB_API_BASE` / `GITHUB_WEB_BASE`)だけ。
/// トークン・秘密鍵をエラー文やログに出さない。
use anyhow::{anyhow, Context, Result};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const MAX_REPO_PAGES: u32 = 5;

pub fn api_base() -> String {
    std::env::var("GITHUB_API_BASE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "https://api.github.com".to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn web_base() -> String {
    std::env::var("GITHUB_WEB_BASE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "https://github.com".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("SENN")
        .build()
        .context("HTTP クライアントの作成に失敗")
}

#[derive(Debug, Deserialize)]
pub struct ManifestOwner {
    pub login: String,
}

/// `POST /app-manifests/{code}/conversions` の応答(使う項目だけ)
#[derive(Debug, Deserialize)]
pub struct ManifestConversion {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub html_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub webhook_secret: String,
    pub pem: String,
    pub owner: Option<ManifestOwner>,
}

#[derive(Debug, Deserialize)]
pub struct InstallationAccount {
    pub login: String,
    #[serde(rename = "type")]
    pub account_type: String,
}

#[derive(Debug, Deserialize)]
pub struct InstallationInfo {
    pub id: i64,
    pub account: InstallationAccount,
    pub repository_selection: String,
    pub suspended_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RepoInfo {
    pub full_name: String,
    pub html_url: String,
    pub private: bool,
}

#[derive(Deserialize)]
struct RepoPage {
    repositories: Vec<RepoInfo>,
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Serialize)]
struct AppClaims {
    iat: i64,
    exp: i64,
    iss: i64,
}

/// コード(Manifest 作成後に GitHub が渡す、1 時間・1 回限り)を App の認証情報に交換する。
pub async fn exchange_manifest_code(code: &str) -> Result<ManifestConversion> {
    if code.is_empty()
        || !code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(anyhow!("不正な code"));
    }
    let url = format!("{}/app-manifests/{}/conversions", api_base(), code);
    let resp = http()?
        .post(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("GitHub への接続に失敗")?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "GitHub が code の交換を拒否しました: {}",
            resp.status()
        ));
    }
    resp.json().await.context("GitHub の応答を読めませんでした")
}

/// App として API を呼ぶための JWT(RS256、有効 9 分)
pub fn app_jwt(app_id: i64, private_key_pem: &str) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    let claims = AppClaims {
        iat: now - 60,
        exp: now + 9 * 60,
        iss: app_id,
    };
    let key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
        .map_err(|_| anyhow!("App の秘密鍵を読めませんでした"))?;
    encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|_| anyhow!("App の JWT を作れませんでした"))
}

pub async fn get_installation(jwt: &str, installation_id: i64) -> Result<InstallationInfo> {
    let url = format!("{}/app/installations/{}", api_base(), installation_id);
    let resp = http()?
        .get(url)
        .bearer_auth(jwt)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("GitHub への接続に失敗")?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "インストール情報を取得できません: {}",
            resp.status()
        ));
    }
    resp.json().await.context("GitHub の応答を読めませんでした")
}

pub async fn installation_token(jwt: &str, installation_id: i64) -> Result<String> {
    let url = format!(
        "{}/app/installations/{}/access_tokens",
        api_base(),
        installation_id
    );
    let resp = http()?
        .post(url)
        .bearer_auth(jwt)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("GitHub への接続に失敗")?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "アクセストークンを取得できません: {}",
            resp.status()
        ));
    }
    let t: TokenResponse = resp
        .json()
        .await
        .context("GitHub の応答を読めませんでした")?;
    Ok(t.token)
}

/// インストールに許可されたリポジトリ一覧(最大 500 件で打ち切り)
pub async fn list_repositories(token: &str) -> Result<Vec<RepoInfo>> {
    let client = http()?;
    let mut all = Vec::new();
    for page in 1..=MAX_REPO_PAGES {
        let url = format!(
            "{}/installation/repositories?per_page=100&page={}",
            api_base(),
            page
        );
        let resp = client
            .get(url)
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .context("GitHub への接続に失敗")?;
        if !resp.status().is_success() {
            return Err(anyhow!("リポジトリ一覧を取得できません: {}", resp.status()));
        }
        let body: RepoPage = resp
            .json()
            .await
            .context("GitHub の応答を読めませんでした")?;
        let n = body.repositories.len();
        all.extend(body.repositories);
        if n < 100 {
            break;
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

    /// テストごとに使い捨ての鍵を作る(鍵をリポジトリに置かない)。openssl が無ければ None。
    fn generate_test_keys() -> Option<(String, String)> {
        let dir = std::env::temp_dir().join(format!("senn-test-key-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).ok()?;
        let priv_path = dir.join("k.pem");
        let ok = std::process::Command::new("openssl")
            .args(["genrsa", "-out"])
            .arg(&priv_path)
            .arg("2048")
            .output()
            .ok()?
            .status
            .success();
        if !ok {
            return None;
        }
        let public = std::process::Command::new("openssl")
            .args(["rsa", "-pubout", "-in"])
            .arg(&priv_path)
            .output()
            .ok()?;
        let private_pem = std::fs::read_to_string(&priv_path).ok()?;
        let _ = std::fs::remove_dir_all(&dir);
        Some((private_pem, String::from_utf8(public.stdout).ok()?))
    }

    #[derive(Deserialize)]
    struct Claims {
        iat: i64,
        exp: i64,
        iss: i64,
    }

    #[test]
    fn app_jwt_is_rs256_with_short_lifetime() {
        let Some((private_pem, public_pem)) = generate_test_keys() else {
            eprintln!("openssl が無いためスキップ");
            return;
        };
        let jwt = app_jwt(12345, &private_pem).unwrap();
        let mut v = Validation::new(Algorithm::RS256);
        v.required_spec_claims.clear();
        let data = decode::<Claims>(
            &jwt,
            &DecodingKey::from_rsa_pem(public_pem.as_bytes()).unwrap(),
            &v,
        )
        .unwrap();
        assert_eq!(data.claims.iss, 12345);
        assert!(data.claims.exp - data.claims.iat <= 10 * 60);
        assert!(data.claims.iat < chrono::Utc::now().timestamp());
    }

    #[test]
    fn app_jwt_rejects_garbage_key_without_leaking_it() {
        let err = app_jwt(1, "not a pem").unwrap_err().to_string();
        assert!(!err.contains("not a pem"));
    }

    #[tokio::test]
    async fn exchange_rejects_odd_codes_before_any_request() {
        assert!(exchange_manifest_code("").await.is_err());
        assert!(exchange_manifest_code("../x").await.is_err());
    }
}
