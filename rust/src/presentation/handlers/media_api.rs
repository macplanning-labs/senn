/// presentation/handlers/media_api.rs — 添付ファイル・カスタム絵文字(/media)の認証付き配信
///
/// 以前は /media が認証なしで配信され、ファイルのURLを知っていれば Private なチケットの添付でも読めた。
/// SPA の認証は Bearer ヘッダーだけで `<img src>` からは送れず、しかもチケット本文に `/media/...` の
/// URLが保存されている(URLは変えられない)。そのため次の方式にする:
///
/// 1. `POST /api/v1/media-session/`(Bearer認証)が、/media 専用の短命Cookieを発行する
/// 2. `GET /media/*path` は、そのCookieを検証し、パスごとに閲覧権を確認してから配信する
///
/// Cookie には種別 `media` のトークン(ユーザーIDのみ)を入れる。API の認証には使えない。
/// 判定に通らないものは 404 にして、チケットや添付の存在を推測させない(Cookieが無い/不正は 401)。
use std::path::{Component, Path as FsPath, PathBuf};

use axum::{
    extract::{Path, Request, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::{CookieJar, SameSite};
use sqlx::PgPool;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::domain::access::{can, Action, Decision, Principal, Viewer};
use crate::domain::services::jwt_service;
use crate::infrastructure::access::{
    facts_repo,
    shadow::{self, Direction, Mode, Resource},
    viewer_repo,
};
use crate::infrastructure::repositories::{membership_repo, user_repo};
use crate::presentation::middleware::auth::build_cookie;
use crate::presentation::state::AppState;

pub const MEDIA_COOKIE: &str = "senn_media";
/// メディアCookieの有効期限(分)。配信のたびに閲覧権をDBで確認するので、権限が外れれば即時に閉じる。
pub const MEDIA_COOKIE_MINUTES: i64 = 60;

/// 配信を許可するパスの種別(許可リスト。これ以外は 404)
#[derive(Debug, PartialEq, Eq)]
pub enum MediaTarget {
    /// attachments/{チケットID}/{ファイル名}
    Attachment { ticket_id: i32 },
    /// custom-emojis/{プロジェクトID}/{ファイル名}
    CustomEmoji { project_id: i32 },
}

/// パスを許可リストに照らして分類する。`..` や空要素・区切り文字の混入は None。
pub fn classify(rel: &str) -> Option<MediaTarget> {
    if rel.is_empty() || rel.contains('\\') || rel.contains('\0') || rel.starts_with('/') {
        return None;
    }
    let parts: Vec<&str> = rel.split('/').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == "..")
    {
        return None;
    }
    let id: i32 = parts[1].parse().ok().filter(|n| *n > 0)?;
    match parts[0] {
        "attachments" => Some(MediaTarget::Attachment { ticket_id: id }),
        "custom-emojis" => Some(MediaTarget::CustomEmoji { project_id: id }),
        _ => None,
    }
}

/// media_dir の配下にある実在ファイルの実パスを返す(シンボリックリンク等で外に出るものは None)。
pub fn resolve_under(media_dir: &FsPath, rel: &str) -> Option<PathBuf> {
    // 相対パスの通常要素だけを許す(念のための二重防御)
    if FsPath::new(rel)
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return None;
    }
    let root = media_dir.canonicalize().ok()?;
    let full = root.join(rel).canonicalize().ok()?;
    (full.starts_with(&root) && full.is_file()).then_some(full)
}

/// そのユーザーが、この対象を見てよいか。
pub async fn authorize(pool: &PgPool, user_id: i32, target: &MediaTarget) -> anyhow::Result<bool> {
    // 今の判定: 添付はチケットの閲覧権、絵文字はログイン済みなら誰でも
    let legacy = match target {
        MediaTarget::Attachment { ticket_id } => {
            membership_repo::check_ticket_access(pool, *ticket_id, user_id).await?
        }
        MediaTarget::CustomEmoji { .. } => true,
    };
    // 新しい判定(アクセス制御の再設計 C-7): 添付はチケット、絵文字はプロジェクトが見えること。試運転のスイッチに従う
    let (resource, resource_id) = match target {
        MediaTarget::Attachment { ticket_id } => (Resource::Ticket, *ticket_id),
        MediaTarget::CustomEmoji { project_id } => (Resource::Project, *project_id),
    };
    let mode = shadow::mode(resource);
    if mode == Mode::Off {
        return Ok(legacy);
    }
    let facts = match target {
        MediaTarget::Attachment { ticket_id } => {
            facts_repo::facts_for_ticket(pool, *ticket_id).await?
        }
        MediaTarget::CustomEmoji { project_id } => {
            facts_repo::facts_for_project(pool, *project_id).await?
        }
    };
    let viewer =
        viewer_repo::load(pool, Principal::Human { user_id }, viewer_repo::today_utc()).await?;
    let new = matches!((&viewer, &facts), (Some(v), Some(f)) if can(v, Action::Read, f) == Decision::Allow);
    if mode == Mode::On {
        return Ok(new);
    }
    if legacy != new {
        let dir = if legacy {
            Direction::NewlyHidden
        } else {
            Direction::NewlyVisible
        };
        shadow::record(
            pool,
            resource,
            Some(user_id),
            "GET /media/*",
            vec![(dir, resource_id as i64)],
        );
    }
    Ok(legacy)
}

/// メディア用Cookieを発行する  POST /api/v1/media-session/
pub async fn issue_session(State(state): State<AppState>, viewer: Viewer) -> Response {
    // 人の閲覧者だけに発行する(配信のたびに、添付の親チケットを新しい判定で確認する。C-6)
    let user_id = match viewer.require_user_id() {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let token = match jwt_service::issue_media_token(
        user_id,
        &state.config.jwt_secret,
        MEDIA_COOKIE_MINUTES,
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("media token issue failed: {:?}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let cookie = build_cookie(
        MEDIA_COOKIE,
        token,
        "/media",
        MEDIA_COOKIE_MINUTES * 60,
        SameSite::Lax,
        state.config.cookie_secure,
    );
    (CookieJar::new().add(cookie), StatusCode::NO_CONTENT).into_response()
}

/// 添付ファイル・絵文字の配信  GET /media/{*path}
pub async fn serve(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(rel): Path<String>,
    req: Request,
) -> Response {
    // 許可リスト外のパスは、認証の有無にかかわらず 404
    let Some(target) = classify(&rel) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Cookie → ユーザー。無い・不正・期限切れ・無効なユーザーは 401
    let user_id = match jar
        .get(MEDIA_COOKIE)
        .and_then(|c| jwt_service::decode_media_token(c.value(), &state.config.jwt_secret).ok())
        .and_then(|claims| claims.user_id().ok())
    {
        Some(id) => id,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    match user_repo::find_by_id(&state.pool, user_id).await {
        Ok(Some(u)) if u.is_active => {}
        Ok(_) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(e) => {
            tracing::error!("media user lookup failed: {:?}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    // 閲覧権の確認。見られない場合は 404(存在を漏らさない)
    match authorize(&state.pool, user_id, &target).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("media authorize failed: {:?}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    let Some(path) = resolve_under(FsPath::new(&state.config.media_dir), &rel) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Range・Content-Type・If-Modified-Since などは ServeFile に任せる
    let mut resp = match ServeFile::new(path).oneshot(req).await {
        Ok(r) => r.into_response(),
        Err(e) => match e {},
    };
    let h = resp.headers_mut();
    // ブラウザの共有でない(private)キャッシュだけ許す。1時間で権限の変更が反映される
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // アップロードされた HTML/SVG 内のスクリプトを、このオリジンで動かさない
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox"),
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        create_test_project, create_test_team, create_test_ticket, create_test_user, test_pool,
    };

    #[test]
    fn classify_allows_only_attachments_and_custom_emojis() {
        assert_eq!(
            classify("attachments/12/abc_file.png"),
            Some(MediaTarget::Attachment { ticket_id: 12 })
        );
        assert_eq!(
            classify("custom-emojis/3/x.png"),
            Some(MediaTarget::CustomEmoji { project_id: 3 })
        );
        for bad in [
            "",
            "/attachments/1/a.png",
            "attachments/1",
            "attachments/1/a/b.png",
            "attachments/../etc/passwd",
            "attachments/1/..",
            "attachments/./1/a.png",
            "attachments/1//a.png",
            "attachments/0/a.png",
            "attachments/-1/a.png",
            "attachments/x/a.png",
            "attachments/1/a\\b.png",
            "static/1/a.png",
            "other/1/a.png",
            ".env",
        ] {
            assert_eq!(classify(bad), None, "許可されてはいけない: {bad:?}");
        }
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("senn-media-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn resolve_stays_inside_media_dir() {
        let root = unique_dir("root");
        std::fs::create_dir_all(root.join("attachments/1")).unwrap();
        std::fs::write(root.join("attachments/1/a.txt"), "ok").unwrap();
        let outside = unique_dir("outside");
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();

        assert!(resolve_under(&root, "attachments/1/a.txt").is_some());
        // 存在しない・ディレクトリ
        assert!(resolve_under(&root, "attachments/1/none.txt").is_none());
        assert!(resolve_under(&root, "attachments/1").is_none());
        // `..` での脱出
        assert!(resolve_under(&root, "attachments/1/../../../secret.txt").is_none());
        // 絶対パス
        assert!(resolve_under(&root, outside.join("secret.txt").to_str().unwrap()).is_none());
        // シンボリックリンクでの脱出
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                outside.join("secret.txt"),
                root.join("attachments/1/link.txt"),
            )
            .unwrap();
            assert!(resolve_under(&root, "attachments/1/link.txt").is_none());
        }
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    #[tokio::test]
    async fn attachment_is_visible_to_team_members_only() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let team = create_test_team(&pool, "media").await;
        let member = create_test_user(&pool, "mm").await;
        let outsider = create_test_user(&pool, "mo").await;
        sqlx::query(
            "INSERT INTO t_team_membership (team_id, user_id, role, joined_at) VALUES ($1, $2, 'member', NOW())",
        )
        .bind(team as i64)
        .bind(member as i64)
        .execute(&pool)
        .await
        .unwrap();
        let project = create_test_project(&pool, "MD", member).await;
        sqlx::query("INSERT INTO tickets_project_teams (project_id, team_id) VALUES ($1, $2)")
            .bind(project as i64)
            .bind(team as i64)
            .execute(&pool)
            .await
            .unwrap();
        let ticket = create_test_ticket(&pool, project, "MD", member).await;
        sqlx::query("UPDATE tickets_ticket SET team_id = $1 WHERE id = $2")
            .bind(team as i64)
            .bind(ticket as i64)
            .execute(&pool)
            .await
            .unwrap();

        let target = MediaTarget::Attachment { ticket_id: ticket };
        assert!(authorize(&pool, member, &target).await.unwrap());
        assert!(!authorize(&pool, outsider, &target).await.unwrap());
        // 存在しないチケットは誰にも見えない
        let none = MediaTarget::Attachment {
            ticket_id: i32::MAX,
        };
        assert!(!authorize(&pool, member, &none).await.unwrap());
    }
}
