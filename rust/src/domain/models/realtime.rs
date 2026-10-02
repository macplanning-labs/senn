//! Realtime sync WebSocket packet types
//!
//! Types for WebSocket-based realtime synchronization of entity changes.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::sync_api::SyncAccessOut;

// ---------------------------------------------------------------------------
// Room Identification
// ---------------------------------------------------------------------------

/// Room identifier for WebSocket subscriptions: "t:3", "p:3:12", "all", "u:17"
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoomId(pub String);

impl RoomId {
    /// Create a team room: "t:{team_id}"
    pub fn team(t: i32) -> Self {
        RoomId(format!("t:{}", t))
    }

    /// Create a project room: "p:{team_id}:{project_id}"
    pub fn project(t: i32, p: i32) -> Self {
        RoomId(format!("p:{}:{}", t, p))
    }

    /// Create the all/broadcast room
    pub fn all() -> Self {
        RoomId("all".to_string())
    }

    /// Create a user-private room: "u:{user_id}"
    pub fn user(u: i32) -> Self {
        RoomId(format!("u:{}", u))
    }
}

/// Room subscription state: room identifier and last known sequence number
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomPosition {
    pub room: RoomId,
    pub seq: u64,
}

// ---------------------------------------------------------------------------
// Change Events
// ---------------------------------------------------------------------------

/// Origin metadata for delta changes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    pub user_id: Option<i32>,
    pub client_request_id: Option<String>,
}

/// Entity change operation, internally tagged by lowercase "op" variant name
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Change {
    /// Insert or update entity
    #[serde(rename_all = "camelCase")]
    Upsert {
        entity: String,
        id: i64,
        v: i64,
        data: Value,
    },
    /// 行が削除された（id と削除時点の版）
    #[serde(rename_all = "camelCase")]
    Delete { entity: String, id: i64, v: i64 },
    /// この部屋からは見えなくなった（削除ではない）
    #[serde(rename_all = "camelCase")]
    Evict { entity: String, id: i64, v: i64 },
    /// 実データは送らない。端末が取り直す（大きい行・利用者ごとに値が変わる行）
    #[serde(rename_all = "camelCase")]
    Stale { entity: String, id: i64, v: i64 },
}

// ---------------------------------------------------------------------------
// Server Packets
// ---------------------------------------------------------------------------

/// WebSocket packets sent by server to client, internally tagged by lowercase "type" variant name
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerPacket {
    /// Initial handshake with connection metadata and access info
    #[serde(rename_all = "camelCase")]
    Welcome {
        epoch: String,
        connection_id: String,
        rooms: Vec<RoomPosition>,
        access: SyncAccessOut,
        server_time: String,
    },
    /// Delta changes for a room
    #[serde(rename_all = "camelCase")]
    Delta {
        epoch: String,
        room: RoomId,
        seq: u64,
        changes: Vec<Change>,
        #[serde(skip_serializing_if = "Option::is_none")]
        origin: Option<Origin>,
    },
    /// Resync request (client data may be stale)
    #[serde(rename_all = "camelCase")]
    Resync {
        epoch: String,
        room: RoomId,
        seq: u64,
        reason: String,
        entities: Vec<String>,
    },
    /// Access rights changed
    #[serde(rename_all = "camelCase")]
    Access {
        access: SyncAccessOut,
        rooms: Vec<RoomPosition>,
    },
    /// Server keepalive ping
    Ping { t: i64 },
}

// ---------------------------------------------------------------------------
// Client Packets
// ---------------------------------------------------------------------------

/// WebSocket packets sent by client to server (Deserialize only), internally tagged by lowercase "type"
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ClientPacket {
    /// Resume subscriptions from epoch/seq or start fresh
    #[serde(rename_all = "camelCase")]
    Resume {
        epoch: Option<String>,
        rooms: Vec<RoomPosition>,
    },
    /// Client keepalive pong response
    Pong { t: i64 },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_room_id_constructors() {
        assert_eq!(RoomId::team(3).0, "t:3");
        assert_eq!(RoomId::project(3, 12).0, "p:3:12");
        assert_eq!(RoomId::all().0, "all");
        assert_eq!(RoomId::user(17).0, "u:17");
    }

    #[test]
    fn test_room_id_serialization() {
        let rid = RoomId::team(3);
        let json_str = serde_json::to_string(&rid).unwrap();
        assert_eq!(json_str, r#""t:3""#);

        let deserialized: RoomId = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized, rid);
    }

    #[test]
    fn test_delta_with_upsert_and_delete_no_origin() {
        // Delta with no origin should omit the "origin" key
        let changes = vec![
            Change::Upsert {
                entity: "ticket".to_string(),
                id: 1,
                v: 2,
                data: json!({"title": "test"}),
            },
            Change::Delete {
                entity: "comment".to_string(),
                id: 5,
                v: 1,
            },
        ];

        let packet = ServerPacket::Delta {
            epoch: "ep123".to_string(),
            room: RoomId::project(3, 12),
            seq: 42,
            changes,
            origin: None,
        };

        let serialized = serde_json::to_value(&packet).unwrap();

        // Verify structure
        assert_eq!(serialized["type"], "delta");
        assert_eq!(serialized["epoch"], "ep123");
        assert_eq!(serialized["room"], "p:3:12");
        assert_eq!(serialized["seq"], 42);
        assert!(serialized.get("origin").is_none()); // origin should not be present

        // Verify changes
        assert_eq!(serialized["changes"].as_array().unwrap().len(), 2);
        assert_eq!(serialized["changes"][0]["op"], "upsert");
        assert_eq!(serialized["changes"][0]["entity"], "ticket");
        assert_eq!(serialized["changes"][1]["op"], "delete");
        assert_eq!(serialized["changes"][1]["entity"], "comment");
    }

    #[test]
    fn test_delta_with_origin_some() {
        // Delta with origin should include userId and clientRequestId in camelCase
        let changes = vec![Change::Upsert {
            entity: "ticket".to_string(),
            id: 1,
            v: 2,
            data: json!({"title": "test"}),
        }];

        let packet = ServerPacket::Delta {
            epoch: "ep123".to_string(),
            room: RoomId::team(3),
            seq: 100,
            changes,
            origin: Some(Origin {
                user_id: Some(17),
                client_request_id: Some("req-abc".to_string()),
            }),
        };

        let serialized = serde_json::to_value(&packet).unwrap();

        // Verify origin fields are camelCase
        assert_eq!(serialized["origin"]["userId"], 17);
        assert_eq!(serialized["origin"]["clientRequestId"], "req-abc");
    }

    #[test]
    fn test_welcome_packet() {
        let access = SyncAccessOut {
            all: true,
            team_ids: vec![1, 2, 3],
            scoped_projects: vec![],
        };

        let packet = ServerPacket::Welcome {
            epoch: "ep123".to_string(),
            connection_id: "conn-xyz".to_string(),
            rooms: vec![RoomPosition {
                room: RoomId::team(1),
                seq: 50,
            }],
            access,
            server_time: "2026-09-30T12:00:00Z".to_string(),
        };

        let serialized = serde_json::to_value(&packet).unwrap();

        assert_eq!(serialized["type"], "welcome");
        assert_eq!(serialized["connectionId"], "conn-xyz");
        assert_eq!(serialized["serverTime"], "2026-09-30T12:00:00Z");
        assert_eq!(serialized["access"]["all"], true);
    }

    #[test]
    fn test_resync_packet() {
        let packet = ServerPacket::Resync {
            epoch: "ep123".to_string(),
            room: RoomId::project(5, 10),
            seq: 200,
            reason: "divergence_detected".to_string(),
            entities: vec!["ticket".to_string(), "comment".to_string()],
        };

        let serialized = serde_json::to_value(&packet).unwrap();

        assert_eq!(serialized["type"], "resync");
        assert_eq!(serialized["room"], "p:5:10");
        assert_eq!(serialized["seq"], 200);
        assert_eq!(serialized["reason"], "divergence_detected");
        assert_eq!(serialized["entities"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_ping_packet() {
        let packet = ServerPacket::Ping { t: 1725052800 };

        let serialized = serde_json::to_value(&packet).unwrap();

        assert_eq!(serialized["type"], "ping");
        assert_eq!(serialized["t"], 1725052800);
    }

    #[test]
    fn test_client_resume_deserialization() {
        let json_str = r#"{"type":"resume","epoch":null,"rooms":[{"room":"t:3","seq":5}]}"#;
        let packet: ClientPacket = serde_json::from_str(json_str).unwrap();

        match packet {
            ClientPacket::Resume { epoch, rooms } => {
                assert_eq!(epoch, None);
                assert_eq!(rooms.len(), 1);
                assert_eq!(rooms[0].room, RoomId::team(3));
                assert_eq!(rooms[0].seq, 5);
            }
            _ => panic!("Expected Resume variant"),
        }
    }

    #[test]
    fn test_client_pong_deserialization() {
        let json_str = r#"{"type":"pong","t":1}"#;
        let packet: ClientPacket = serde_json::from_str(json_str).unwrap();

        match packet {
            ClientPacket::Pong { t } => {
                assert_eq!(t, 1);
            }
            _ => panic!("Expected Pong variant"),
        }
    }

    #[test]
    fn test_client_unknown_type_error() {
        let json_str = r#"{"type":"unknown","data":"test"}"#;
        let result: Result<ClientPacket, _> = serde_json::from_str(json_str);

        assert!(result.is_err(), "Unknown type should fail deserialization");
    }

    #[test]
    fn test_change_evict_variant() {
        let change = Change::Evict {
            entity: "ticket".to_string(),
            id: 42,
            v: 3,
        };

        let serialized = serde_json::to_value(&change).unwrap();

        assert_eq!(serialized["op"], "evict");
        assert_eq!(serialized["entity"], "ticket");
        assert_eq!(serialized["id"], 42);
        assert_eq!(serialized["v"], 3);
    }

    #[test]
    fn test_change_stale_variant() {
        let change = Change::Stale {
            entity: "comment".to_string(),
            id: 100,
            v: 5,
        };

        let serialized = serde_json::to_value(&change).unwrap();

        assert_eq!(serialized["op"], "stale");
        assert_eq!(serialized["entity"], "comment");
        assert_eq!(serialized["id"], 100);
        assert_eq!(serialized["v"], 5);
    }

    #[test]
    fn test_room_position_roundtrip() {
        let room_pos = RoomPosition {
            room: RoomId::project(7, 14),
            seq: 999,
        };

        let json = serde_json::to_value(&room_pos).unwrap();
        let deserialized: RoomPosition = serde_json::from_value(json).unwrap();

        assert_eq!(deserialized.room, room_pos.room);
        assert_eq!(deserialized.seq, room_pos.seq);
    }
}
