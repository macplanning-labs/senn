/**
 * lww.ts — Last Write Wins (LWW) 競合解決
 *
 * サーバーからの変更（incomingV）とローカル状態（local）の競合判定。
 * ローカル未送信の変更がある場合は優先し、そうでなければバージョン比較。
 *
 * 詳細設計 §4.1（競合解決ロジック）
 */

/** バージョン管理とダーティフラグを持つローカルエンティティ */
export interface LocalVersioned {
  /** バージョン番号（サーバー値；undefined = ローカル作成） */
  v?: number;
  /** 未送信の編集フラグ */
  _dirty?: boolean;
  /** 作成送信待ちフラグ */
  _pendingCreate?: boolean;
  /** 削除送信待ちフラグ */
  _deleted?: boolean;
}

/** 競合解決の判定結果 */
export type Verdict = 'apply' | 'skip';

/**
 * 受け取ったサーバー値（incomingV）をローカルに反映させるべきか判定
 *
 * @param local - ローカル行（存在しない場合は undefined）
 * @param incomingV - サーバーバージョン
 * @param tomb - ローカルで削除済みの墓石レコード（削除が未送信の場合の復帰防止用）
 * @returns 'apply' = 反映させる、'skip' = ローカル値を優先
 *
 * 優先順位（いずれか該当したら以降を評価しない）：
 * 1. local が存在し、_dirty/_pendingCreate/_deleted いずれかが真 → skip
 * 2. local が undefined のとき、tomb が存在して tomb.v >= incomingV → skip（削除優先）
 * 3. それ以外、バージョン比較：(local.v ?? 0) < incomingV → apply、そうでなければ skip
 */
export function shouldApply(local: LocalVersioned | undefined, incomingV: number, tomb: { v: number } | undefined): Verdict {
  // ルール1: ローカル未送信の変更がある場合は優先
  if (local !== undefined && (local._dirty || local._pendingCreate || local._deleted)) {
    return 'skip';
  }

  // ルール2: ローカルが存在しない場合、削除済み墓石で防ぐ
  if (local === undefined) {
    if (tomb !== undefined && tomb.v >= incomingV) {
      return 'skip';
    }
    return 'apply';
  }

  // ルール3: バージョン比較（local が存在し、未送信フラグなし）
  const localV = local.v ?? 0;
  if (localV < incomingV) {
    return 'apply';
  }
  return 'skip';
}
