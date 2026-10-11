/**
 * 認証情報の読み込み(MCP サーバーと senn-ai-run で共通)。
 *
 * 読む順: 環境変数 → ~/.config/senn-ai-agent/credentials.env(KEY=VALUE 形式)。
 * 接続先(SENN_BASE_URL)に既定値は持たない。自分の SENN の URL を必ず指定する。
 * Cloudflare Access のサービストークン(SENN_CF_ACCESS_*)は任意。使う環境だけ設定する。
 */
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export interface RawCredentials {
  baseUrl?: string;
  cfAccessClientId?: string;
  cfAccessClientSecret?: string;
  aiApiKey?: string;
}

/** 認証情報のフォルダ(既定は ~/.config/senn-ai-agent) */
export function credentialsDir(): string {
  return join(homedir(), ".config", "senn-ai-agent");
}

/** 旧名のフォルダ(移行中のみ読む) */
function legacyCredentialsDir(): string {
  return join(homedir(), ".config", "wip-ai-agent");
}

/** credentials.env をパースする(KEY=VALUE 形式。無い・読めないときは空) */
export function parseCredentialsFile(path: string): Record<string, string> {
  try {
    const content = readFileSync(path, "utf-8");
    const result: Record<string, string> = {};
    for (const line of content.split("\n")) {
      const trimmed = line.trim().replace(/^export\s+/, "");
      if (trimmed.startsWith("#") || trimmed === "") continue;
      const [key, ...valueParts] = trimmed.split("=");
      if (!key || valueParts.length === 0) continue;
      let value = valueParts.join("=").trim();
      // クォートを外す
      if (
        (value.startsWith('"') && value.endsWith('"')) ||
        (value.startsWith("'") && value.endsWith("'"))
      ) {
        value = value.slice(1, -1);
      }
      result[key.trim()] = value;
    }
    return result;
  } catch {
    return {};
  }
}

/** SENN_* を優先し、移行中のみ旧 WIP_* も読む */
function pick(src: Record<string, string | undefined>, name: string): string | undefined {
  return src[`SENN_${name}`] || src[`WIP_${name}`] || undefined;
}

/**
 * 環境変数と credentials.env から、認証情報を集める(値の検証はしない)。
 * readFile=false のとき(クラウド実行など)は、環境変数だけを読む。
 */
export function collectCredentials(opts: { readFile: boolean }): RawCredentials {
  const env = process.env as Record<string, string | undefined>;
  let file: Record<string, string> = {};
  if (opts.readFile) {
    file = parseCredentialsFile(join(credentialsDir(), "credentials.env"));
    if (Object.keys(file).length === 0) {
      file = parseCredentialsFile(join(legacyCredentialsDir(), "credentials.env"));
    }
  }
  const get = (name: string) => pick(env, name) || pick(file, name);
  return {
    baseUrl: get("BASE_URL"),
    cfAccessClientId: get("CF_ACCESS_CLIENT_ID"),
    cfAccessClientSecret: get("CF_ACCESS_CLIENT_SECRET"),
    aiApiKey: get("AI_API_KEY"),
  };
}

export const BASE_URL_MISSING_MESSAGE =
  "接続先の SENN が設定されていません。環境変数 SENN_BASE_URL、または " +
  "~/.config/senn-ai-agent/credentials.env に SENN_BASE_URL=https://(あなたの SENN のホスト) を書いてください。";

/**
 * 接続先の URL を確かめる。http(暗号化なし)で localhost 以外へ送るときは、警告の文を返す。
 * 形が正しくなければ例外。
 */
export function checkBaseUrl(raw: string): { baseUrl: string; warning: string | null } {
  let u: URL;
  try {
    u = new URL(raw);
  } catch {
    throw new Error(`SENN_BASE_URL の形が正しくありません(http:// または https:// で始まる URL を指定してください)。`);
  }
  if (u.protocol !== "https:" && u.protocol !== "http:") {
    throw new Error(`SENN_BASE_URL は http:// または https:// で始まる URL を指定してください。`);
  }
  const baseUrl = raw.replace(/\/+$/, "");
  const local = ["localhost", "127.0.0.1", "[::1]"].includes(u.hostname) || u.hostname.endsWith(".localhost");
  const warning =
    u.protocol === "http:" && !local
      ? `警告: SENN_BASE_URL が http(暗号化なし)です。AI 用のキーやチケットの内容が、通信経路で読まれるおそれがあります。https を使ってください。`
      : null;
  return { baseUrl, warning };
}
