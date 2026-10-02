//! Sync API types for local-first synchronization
//!
//! Types for the differential sync API (/api/v1/sync/tickets/, /api/v1/sync/projects/)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Re-export common types
pub use crate::domain::models::resource_api::ProjectOut;
pub use crate::domain::models::ticket_api::TicketListOut;

// ---------------------------------------------------------------------------
// Sync Data Transfer Objects
// ---------------------------------------------------------------------------

/// Extended ticket data for sync (TicketListOut + description + closedAt)
#[derive(Debug, Clone, Serialize)]
pub struct TicketSyncOut {
    #[serde(flatten)]
    pub base: TicketListOut,
    pub description: String,
    #[serde(rename = "closedAt")]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(rename = "aiPrompt")]
    pub ai_prompt: Option<String>,
    #[serde(rename = "aiPromptUpdatedAt")]
    pub ai_prompt_updated_at: Option<DateTime<Utc>>,
    #[serde(rename = "aiPromptGenerationMode")]
    pub ai_prompt_generation_mode: Option<String>,
    /// 行の版番号（sync_version）。端末は手元より新しい版のときだけ上書きする
    pub v: i64,
}

/// コメントの同期用 DTO。閲覧者に依存する値（canEdit 等）と、他の行から導ける値（replyCount）は持たない。
/// 端末が「本人か」「担当者か」から編集可否を導き、返信数は端末内の子コメントから数える。
#[derive(Debug, Clone, Serialize)]
pub struct CommentSyncOut {
    pub id: i32,
    #[serde(rename = "ticketId")]
    pub ticket_id: i32,
    /// 論理削除済みは空文字
    pub body: String,
    pub author: crate::domain::models::ticket_api::UserSummaryOut,
    #[serde(rename = "actingUser")]
    pub acting_user: Option<crate::domain::models::ticket_api::UserSummaryOut>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(rename = "anchorStart")]
    pub anchor_start: Option<i32>,
    #[serde(rename = "anchorEnd")]
    pub anchor_end: Option<i32>,
    #[serde(rename = "anchorQuote")]
    pub anchor_quote: Option<String>,
    #[serde(rename = "parentCommentId")]
    pub parent_comment_id: Option<i32>,
    #[serde(rename = "isDeleted")]
    pub is_deleted: bool,
    /// 投稿者が AI エージェントか（編集可否の導出に使う）
    #[serde(rename = "isAiAgentAuthor")]
    pub is_ai_agent_author: bool,
    /// 行の版番号（sync_version）
    pub v: i64,
}

/// Extended project data for sync (ProjectOut + updatedAt)
#[derive(Debug, Clone, Serialize)]
pub struct ProjectSyncOut {
    #[serde(flatten)]
    pub base: ProjectOut,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
    /// 行の版番号（sync_version）
    pub v: i64,
}

/// Deleted entity record in sync response
#[derive(Debug, Clone, Serialize)]
pub struct SyncDeletedOut {
    pub id: i64,
    /// 見られないチケットは null（キーを漏らさない）
    pub key: Option<String>,
    /// 削除（または見られなくなった）時点の版番号
    pub v: i64,
}

/// User's access rights
#[derive(Debug, Clone, Serialize)]
pub struct SyncAccessOut {
    pub all: bool,
    #[serde(rename = "teamIds")]
    pub team_ids: Vec<i32>,
    #[serde(rename = "scopedProjects")]
    pub scoped_projects: Vec<ScopedProjectOut>,
}

/// Scoped project access (team_id + project_id pair)
#[derive(Debug, Clone, Serialize)]
pub struct ScopedProjectOut {
    #[serde(rename = "teamId")]
    pub team_id: i32,
    #[serde(rename = "projectId")]
    pub project_id: i32,
}

/// Sync response page for a single entity type
#[derive(Debug, Clone, Serialize)]
pub struct SyncPageOut<T> {
    pub changes: Vec<T>,
    pub deleted: Vec<SyncDeletedOut>,
    pub access: SyncAccessOut,
    pub cursor: String,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
    #[serde(rename = "serverTime")]
    pub server_time: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Cursor Encoding/Decoding
// ---------------------------------------------------------------------------

/// Opaque cursor for pagination: tracks position in both changes and deletions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCursor {
    /// sync_changed_at position for changes
    pub c: DateTime<Utc>,
    /// id position for changes
    pub i: i64,
    /// deleted_at position for deletions
    pub d: DateTime<Utc>,
    /// id position for deletions
    pub di: i64,
}

impl SyncCursor {
    /// Encode cursor to base64url string (no padding)
    pub fn encode(&self) -> String {
        match serde_json::to_string(self) {
            Ok(json_str) => {
                use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
                URL_SAFE_NO_PAD.encode(json_str.as_bytes())
            }
            Err(_) => String::new(),
        }
    }

    /// Decode cursor from base64url string
    pub fn decode(s: &str) -> Result<Self, ()> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD
            .decode(s)
            .ok()
            .and_then(|bytes| {
                String::from_utf8(bytes)
                    .ok()
                    .and_then(|json_str| serde_json::from_str(&json_str).ok())
            })
            .ok_or(())
    }
}

// ---------------------------------------------------------------------------
// Error Types
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum SyncError {
    Expired,
    Db(anyhow::Error),
}

impl From<sqlx::Error> for SyncError {
    fn from(err: sqlx::Error) -> Self {
        SyncError::Db(anyhow::Error::from(err))
    }
}

impl From<anyhow::Error> for SyncError {
    fn from(err: anyhow::Error) -> Self {
        SyncError::Db(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_encode_decode() {
        let original = SyncCursor {
            c: DateTime::parse_from_rfc3339("2026-09-25T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            i: 42,
            d: DateTime::parse_from_rfc3339("2026-09-24T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            di: 5,
        };

        let encoded = original.encode();
        let decoded = SyncCursor::decode(&encoded).expect("decode should succeed");

        assert_eq!(original.c, decoded.c);
        assert_eq!(original.i, decoded.i);
        assert_eq!(original.d, decoded.d);
        assert_eq!(original.di, decoded.di);
    }

    #[test]
    fn test_cursor_decode_invalid() {
        let invalid = "not-hex-encoded";
        assert!(SyncCursor::decode(invalid).is_err());
    }
}
