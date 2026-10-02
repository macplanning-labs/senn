//! realtime_api.rs — リアルタイム同期の入口（接続用トークンの発行・WebSocket）
//!
//! 設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §5.4, §7.6
//!
//! 認証: ブラウザの WebSocket は Authorization ヘッダーを付けられない。JWT を URL に載せるとログに残るので、
//! 通常の認証で「30秒で切れる使い捨てトークン」を発行し、それを URL に付けて接続する。
//! トークンは接続時に DELETE ... RETURNING で1回だけ取り出せる（再利用・複数台・再起動をまたいでも1回限り）。
use std::time::Duration;

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::domain::access::Viewer;
use crate::{
    domain::models::realtime::{ClientPacket, ServerPacket},
    infrastructure::{
        realtime::{
            hub::{Frame, CONN_QUEUE},
            rooms::rooms_for_access,
        },
        repositories::sync_repo,
    },
    AppState,
};

/// 接続用トークンの有効期間（秒）
const TOKEN_TTL_SECS: i64 = 30;
/// 最初の resume を待つ時間
const FIRST_MESSAGE_TIMEOUT: Duration = Duration::from_secs(5);
/// ping の間隔（Cloudflare Tunnel の無通信切断より短く）
const PING_INTERVAL: Duration = Duration::from_secs(25);
/// この時間、端末から何も届かなければ切る
const IDLE_LIMIT: Duration = Duration::from_secs(75);
/// 1本の接続の最長時間。過ぎたら切る（端末が新しいトークンでつなぎ直す）。停止した利用者を残さないため
const MAX_LIFETIME: Duration = Duration::from_secs(15 * 60);

/// 読み取りバッファ。resume（受信位置の一覧）が入る大きさ
const WS_READ_BUFFER: usize = 8 * 1024;
/// 書き込みの詰まりの上限。配信する行の本文は最大でも数十KB（64KBを超える行は合図に変える）
const WS_MAX_WRITE_BUFFER: usize = 256 * 1024;
/// 端末から受け取るメッセージの上限（resume / pong だけ）
const WS_MAX_INCOMING_MESSAGE: usize = 64 * 1024;

const CLOSE_TOKEN_EXPIRED: u16 = 4401;
const CLOSE_SLOW: u16 = 4408;

fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[derive(Serialize)]
pub struct ConnectTokenOut {
    token: String,
    #[serde(rename = "expiresIn")]
    expires_in: i64,
}

/// POST /api/v1/realtime/connect-token/
pub async fn connect_token(
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Json<ConnectTokenOut>, StatusCode> {
    // 人の閲覧者だけに発行する(キー経由のリアルタイム接続は受け付けない)
    let user_id = viewer.user_id().ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.realtime_enabled {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    sqlx::query(
        "INSERT INTO realtime_connect_tokens (token_hash, user_id, expires_at)
         VALUES ($1, $2, NOW() + make_interval(secs => $3))",
    )
    .bind(hash_token(&token))
    .bind(user_id)
    .bind(TOKEN_TTL_SECS as f64)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "realtime: token insert failed");
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    Ok(Json(ConnectTokenOut {
        token,
        expires_in: TOKEN_TTL_SECS,
    }))
}

#[derive(Deserialize)]
pub struct WsQuery {
    token: Option<String>,
}

/// GET /api/v1/realtime/ws?token=...
///
/// トークンの消費は WebSocket へのアップグレードより前に、HTTP の段階で行う。
/// 取れなければ 101 を返さず、ここで 401 を返して終わる（不正・期限切れ・2回目を区別しない）。
pub async fn ws(
    State(state): State<AppState>,
    Query(q): Query<WsQuery>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !state.realtime_enabled {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let Some(token) = q.token.filter(|t| !t.is_empty() && t.len() <= 128) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let consumed: Result<Option<i32>, sqlx::Error> = sqlx::query_scalar(
        "DELETE FROM realtime_connect_tokens WHERE token_hash = $1 AND expires_at > NOW() RETURNING user_id",
    )
    .bind(hash_token(&token))
    .fetch_optional(&state.pool)
    .await;
    // DB の障害は 503（401 にすると端末が「ログイン切れ」と受け取って再ログインに向かう）
    let user_id = match consumed {
        Err(e) => {
            tracing::error!(error = %e, "realtime: token consume failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        Ok(None) => {
            tracing::warn!("realtime: connect token rejected");
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Ok(Some(id)) => id,
    };
    // 認証(jwt_auth)と同じ関数を使う(同じ SQL を別の型で渡すと、使い回された準備済みの文で失敗する。DEMO-000164)
    let active =
        crate::infrastructure::repositories::user_repo::is_active(&state.pool, user_id).await;
    match active {
        Ok(Some(true)) => {}
        Ok(_) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "realtime: user lookup failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    }
    // 接続ごとのメモリを絞る。実装（tungstenite）の既定は、読み・書きのバッファがそれぞれ128KiBで、
    // 接続1本あたり約256KBを確保してしまう（負荷試験で確認）。こちらのやり取りは小さいので、足りる大きさにする。
    // 端末から届くのは resume / pong だけなので、メッセージの大きさにも上限を付ける（巨大なメッセージでメモリを食わせる攻撃への備え）。
    upgrade
        .read_buffer_size(WS_READ_BUFFER)
        .write_buffer_size(0) // 書いたらすぐ送る（ためない）
        .max_write_buffer_size(WS_MAX_WRITE_BUFFER)
        .max_message_size(WS_MAX_INCOMING_MESSAGE)
        .max_frame_size(WS_MAX_INCOMING_MESSAGE)
        .on_upgrade(move |socket| serve(socket, state, user_id))
}

fn text(packet: &ServerPacket) -> Message {
    Message::Text(
        serde_json::to_string(packet)
            .expect("serialize ServerPacket")
            .into(),
    )
}

fn close(code: u16, reason: &'static str) -> Message {
    Message::Close(Some(CloseFrame {
        code,
        reason: reason.into(),
    }))
}

async fn serve(socket: WebSocket, state: AppState, user_id: i32) {
    let (mut sink, mut stream) = socket.split();

    // 端末は接続直後に resume を送ってくる（受信位置。初回は epoch なし）。これを待ってから部屋に入る。
    let (epoch, from) = match tokio::time::timeout(FIRST_MESSAGE_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(Message::Text(t)))) => match serde_json::from_str::<ClientPacket>(&t) {
            Ok(ClientPacket::Resume { epoch, rooms }) => (epoch, rooms),
            _ => return,
        },
        _ => return,
    };

    // 購読の範囲は同期と同じ規則(E-2)。無効化されたユーザー(on)は接続を終える
    let access = match sync_repo::realtime_access(&state.pool, user_id).await {
        Ok(Some(a)) => a,
        Ok(None) => return,
        Err(e) => {
            tracing::error!(error = %e, user_id, "realtime: access lookup failed");
            return;
        }
    };
    let rooms = rooms_for_access(&access, user_id);
    let (tx, mut rx) = mpsc::channel::<Frame>(CONN_QUEUE);
    let (cid, positions) = state
        .realtime
        .join_resuming(user_id, tx, rooms, epoch.as_deref(), &from)
        .await;

    // welcome を先に書く。再送・新着は rx に溜まっていて、この後で流れる
    let welcome = ServerPacket::Welcome {
        epoch: state.realtime.epoch.clone(),
        connection_id: cid.to_string(),
        rooms: positions,
        access,
        server_time: chrono::Utc::now().to_rfc3339(),
    };
    if sink.send(text(&welcome)).await.is_err() {
        state.realtime.leave(cid).await;
        return;
    }

    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.tick().await; // 最初の tick は即時なので捨てる
    let lifetime = tokio::time::sleep(MAX_LIFETIME);
    tokio::pin!(lifetime);
    let mut last_seen = tokio::time::Instant::now();

    loop {
        tokio::select! {
            frame = rx.recv() => match frame {
                Some(f) => {
                    if sink.send(Message::Text(f.to_string().into())).await.is_err() { break; }
                }
                None => {
                    // Hub が切断した（送信待ちがあふれた）。端末は再接続して resume する
                    let _ = sink.send(close(CLOSE_SLOW, "slow consumer")).await;
                    break;
                }
            },
            msg = stream.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    last_seen = tokio::time::Instant::now();
                    // pong 以外（2回目の resume など）は無視する
                    if serde_json::from_str::<ClientPacket>(&t).is_err() { break; }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => { last_seen = tokio::time::Instant::now(); }
            },
            _ = ping.tick() => {
                if last_seen.elapsed() > IDLE_LIMIT { break; }
                if sink.send(text(&ServerPacket::Ping { t: chrono::Utc::now().timestamp_millis() })).await.is_err() { break; }
            }
            _ = &mut lifetime => {
                let _ = sink.send(close(CLOSE_TOKEN_EXPIRED, "reconnect")).await;
                break;
            }
        }
    }
    state.realtime.leave(cid).await;
}
