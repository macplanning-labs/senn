// 端末の接続(コピー・貼り付けなし)(DEMO-000192 フェーズ4)
//
// このプログラム(利用者の Mac で動く MCP)が、キーを自分で作り、サーバーには「指紋(SHA-256)」だけを渡す。
// キーの平文は Mac の外に出ない(サーバー・画面・通信・クリップボードに出ない)。
// 承認されたら、このプログラムが、自分のキーを credentials.env に保存し、メモリ上のヘッダも入れ替える。
// 設計: docs/design/詳細設計書_AIAgent_MCP操作の申請.md §8
import { createHash, randomBytes } from "node:crypto";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { homedir, hostname } from "node:os";
import { join } from "node:path";

export const KEY_PREFIX = "senn_ai_";
const STATE_FILE = "pairing.json";
const CREDENTIALS_FILE = "credentials.env";
/** 待ちの状態を捨てるまでの日数(承認が長く来ない場合の打ち切り) */
export const STATE_MAX_AGE_DAYS = 14;

/** 認証情報のフォルダ(既定は ~/.config/senn-ai-agent)。テストでは一時フォルダを渡す */
export function defaultDir(): string {
  return join(homedir(), ".config", "senn-ai-agent");
}

/** 新しいキー(平文)を作る。サーバーの `senn_ai_` + 256bit(base64url) と同じ形 */
export function generateKey(): string {
  return `${KEY_PREFIX}${randomBytes(32).toString("base64url")}`;
}

/** キーの指紋(SHA-256 の16進・小文字)。サーバーの照合と同じ計算 */
export function hashKey(key: string): string {
  return createHash("sha256").update(key, "utf8").digest("hex");
}

/** 一覧表示用の先頭12文字(サーバーの display_prefix と同じ) */
export function keyPrefix12(key: string): string {
  return key.slice(0, 12);
}

/** Mac の名前(ラベル)。 .local を除く */
export function defaultLabel(): string {
  return hostname().replace(/\.local$/i, "").slice(0, 100);
}

export interface PairingState {
  code: string;
  /** サーバーが一度だけ返した、状態の問い合わせ用の秘密(このファイルにだけ置く) */
  deviceSecret: string;
  /** この Mac が作ったキーの平文(承認されるまで、このファイル(権限600)にだけ置く) */
  key: string;
  url: string;
  label: string;
  createdAt: string;
}

function writeFileAtomic(path: string, content: string): void {
  const tmp = `${path}.tmp-${process.pid}`;
  writeFileSync(tmp, content, { mode: 0o600 });
  chmodSync(tmp, 0o600);
  renameSync(tmp, path);
}

export function saveState(dir: string, st: PairingState): void {
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  writeFileAtomic(join(dir, STATE_FILE), JSON.stringify(st));
}

export function loadState(dir: string, now: Date = new Date()): PairingState | null {
  const p = join(dir, STATE_FILE);
  if (!existsSync(p)) return null;
  try {
    const st = JSON.parse(readFileSync(p, "utf8")) as PairingState;
    if (!st.code || !st.deviceSecret || !st.key || !st.createdAt) return null;
    const ageMs = now.getTime() - new Date(st.createdAt).getTime();
    if (Number.isNaN(ageMs) || ageMs > STATE_MAX_AGE_DAYS * 86_400_000) return null;
    return st;
  } catch {
    return null;
  }
}

export function clearState(dir: string): void {
  rmSync(join(dir, STATE_FILE), { force: true });
}

export interface WriteResult {
  written: boolean;
  /** バックアップのパス(値は含まない) */
  backup?: string;
}

/**
 * credentials.env の SENN_AI_API_KEY を、新しいキーに書き換える。
 * バックアップ(.bak-pairing-日時)を取り、一時ファイルに書いて入れ替える(権限600)。
 * credentials.env が無いときは、新しく作る(権限600。接続先 baseUrl が分かれば SENN_BASE_URL も書く)。
 * 作らないと、キーがメモリにしか残らず、再起動で失われるため。
 */
export function updateCredentials(
  dir: string,
  key: string,
  now: Date = new Date(),
  baseUrl?: string
): WriteResult {
  const path = join(dir, CREDENTIALS_FILE);
  if (!existsSync(path)) {
    mkdirSync(dir, { recursive: true, mode: 0o700 });
    const lines = [
      "# SENN MCP の認証情報(端末の接続で自動作成)。このファイルを人に渡さない",
      ...(baseUrl ? [`SENN_BASE_URL=${baseUrl}`] : []),
      `SENN_AI_API_KEY=${key}`,
    ];
    writeFileAtomic(path, `${lines.join("\n")}\n`);
    return { written: true };
  }
  const stamp = now.toISOString().replace(/[-:T]/g, "").slice(0, 14);
  const backup = `${path}.bak-pairing-${stamp}`;
  copyFileSync(path, backup);
  chmodSync(backup, 0o600);

  const original = readFileSync(path, "utf8");
  const line = `SENN_AI_API_KEY=${key}`;
  const pattern = /^(?:export\s+)?SENN_AI_API_KEY=.*$/m;
  let next: string;
  if (pattern.test(original)) {
    next = original.replace(pattern, () => line);
  } else {
    next = original.endsWith("\n") || original === "" ? `${original}${line}\n` : `${original}\n${line}\n`;
  }
  writeFileAtomic(path, next);
  return { written: true, backup };
}

export type RemoteStatus = "waiting" | "requested" | "approved" | "rejected" | "failed" | "expired";

export interface StatusReply {
  status: RemoteStatus;
  message?: string | null;
}

export type Outcome =
  | { kind: "pending"; status: "waiting" | "requested" }
  | { kind: "connected"; wrote: boolean }
  | { kind: "closed"; status: "rejected" | "failed" | "expired"; message: string | null };

/**
 * サーバーから返った状態を、この Mac の状態に反映する。
 * - approved: キーを credentials.env に保存し、ヘッダを入れ替え、待ちの状態を消す
 * - rejected / failed / expired: 待ちの状態を消す(キーは登録されていないので、捨てる)
 * - waiting / requested: 何もしない
 */
export function applyStatus(
  dir: string,
  st: PairingState,
  reply: StatusReply,
  setApiKey: (key: string) => void,
  now: Date = new Date(),
  baseUrl?: string
): Outcome {
  switch (reply.status) {
    case "approved": {
      const r = updateCredentials(dir, st.key, now, baseUrl);
      setApiKey(st.key);
      clearState(dir);
      return { kind: "connected", wrote: r.written };
    }
    case "rejected":
    case "failed":
    case "expired":
      clearState(dir);
      return { kind: "closed", status: reply.status, message: reply.message ?? null };
    default:
      return { kind: "pending", status: reply.status };
  }
}
