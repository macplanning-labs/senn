/**
 * packets.ts — WebSocket リアルタイム同期のパケット型定義
 *
 * 詳細設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §3
 * サーバー（rust/src/domain/models/realtime.rs）が部屋ごとに連番 seq を振って差分を配る。
 * 端末は seq の飛びを見つけたら、既存の差分同期（cursor）で埋める。
 */

import type { SyncAccess } from '../db';

/** エンティティ種別 */
export type EntityName = 'ticket' | 'project' | 'comment' | 'reaction' | 'wiki' | 'attachment' | 'cycle' | 'notification';

/** 部屋 ID。'g' 全員 / 't:チーム' / 'p:チーム:プロジェクト'(期間限定メンバー) / 'all' 管理者 / 'u:利用者' */
export type RoomId = string;

/** 部屋の受信位置（その部屋で最後に適用した seq） */
export interface RoomPosition {
  room: RoomId;
  seq: number;
}

/** 変更操作の型 */
export type Change =
  | { op: 'upsert'; entity: EntityName; id: number; v: number; data: Record<string, unknown> }
  | { op: 'delete'; entity: EntityName; id: number; v: number }
  | { op: 'evict'; entity: EntityName; id: number; v: number }
  | { op: 'stale'; entity: EntityName; id: number; v: number };

/** ウェルカムパケット（接続確立時） */
export interface WelcomePacket {
  type: 'welcome';
  epoch: string;
  connectionId: string;
  rooms: RoomPosition[];
  access: SyncAccess;
  serverTime: string;
}

/** デルタパケット（差分更新） */
export interface DeltaPacket {
  type: 'delta';
  epoch: string;
  room: RoomId;
  seq: number;
  changes: Change[];
  origin?: { userId?: number | null; clientRequestId?: string | null };
}

/** リシンク要求パケット（シーケンス穴埋めが不可な場合） */
export interface ResyncPacket {
  type: 'resync';
  epoch: string;
  room: RoomId;
  seq: number;
  reason: 'buffer_exceeded' | 'epoch_changed' | 'slow_consumer' | 'bulk_change';
  entities: EntityName[];
}

/** アクセス権変更パケット */
export interface AccessPacket {
  type: 'access';
  access: SyncAccess;
  rooms: RoomPosition[];
}

/** キープアライブ（サーバー → クライアント） */
export interface PingPacket {
  type: 'ping';
  t: number;
}

/** サーバーから送信されるパケット */
export type ServerPacket = WelcomePacket | DeltaPacket | ResyncPacket | AccessPacket | PingPacket;

/** クライアントから送信されるパケット */
export type ClientPacket =
  | { type: 'resume'; epoch: string | null; rooms: RoomPosition[] }
  | { type: 'pong'; t: number };

/** 受け取った値が ServerPacket かどうかを判定（型ガード） */
export function isServerPacket(v: unknown): v is ServerPacket {
  if (!v || typeof v !== 'object') return false;
  const type = (v as Record<string, unknown>).type;
  return type === 'welcome' || type === 'delta' || type === 'resync' || type === 'access' || type === 'ping';
}
