import {
  findPlaceholderCredentials,
  placeholderCredentialsMessage,
} from "./credentials-guard.js";
import { BASE_URL_MISSING_MESSAGE, checkBaseUrl, collectCredentials } from "./credentials-file.js";

export interface Config {
  baseUrl: string;
  /** 付けるヘッダ。Cloudflare Access の値・AI 用のキーは、設定されているものだけ */
  headers: Record<string, string>;
}

/**
 * 設定を読む。必須は接続先(SENN_BASE_URL)だけ。
 * - AI 用のキー(SENN_AI_API_KEY)が無くても起動する。そのときは senn_connect_account で
 *   この端末を接続でき、承認されるとキーが自動で保存される(それまでほかのツールは使えない)。
 * - Cloudflare Access のサービストークンは、使う環境だけ設定する(無ければヘッダを付けない)。
 */
export function loadConfig(): Config {
  const c = collectCredentials({ readFile: !process.env.CLAUDE_CODE_REMOTE_SESSION_ID });

  // テスト用の値で上書きされていたら、分かりにくい 403 になる前に止める
  const placeholders = findPlaceholderCredentials({
    SENN_CF_ACCESS_CLIENT_ID: c.cfAccessClientId,
    SENN_CF_ACCESS_CLIENT_SECRET: c.cfAccessClientSecret,
    SENN_AI_API_KEY: c.aiApiKey,
  });
  if (placeholders.length > 0) {
    console.error(
      `エラー: ${placeholderCredentialsMessage(placeholders, "環境変数または ~/.config/senn-ai-agent/credentials.env")}`
    );
    process.exit(1);
  }

  if (!c.baseUrl) {
    console.error(`エラー: ${BASE_URL_MISSING_MESSAGE}`);
    process.exit(1);
  }
  let checked: ReturnType<typeof checkBaseUrl>;
  try {
    checked = checkBaseUrl(c.baseUrl);
  } catch (e) {
    console.error(`エラー: ${e instanceof Error ? e.message : String(e)}`);
    process.exit(1);
  }
  if (checked.warning) console.error(checked.warning);

  // Cloudflare Access の値は、片方だけでは使えない
  if (Boolean(c.cfAccessClientId) !== Boolean(c.cfAccessClientSecret)) {
    console.error(
      "エラー: SENN_CF_ACCESS_CLIENT_ID と SENN_CF_ACCESS_CLIENT_SECRET は、両方設定するか、両方とも設定しないでください。"
    );
    process.exit(1);
  }

  const headers: Record<string, string> = {};
  if (c.cfAccessClientId && c.cfAccessClientSecret) {
    headers["CF-Access-Client-Id"] = c.cfAccessClientId;
    headers["CF-Access-Client-Secret"] = c.cfAccessClientSecret;
  }
  if (c.aiApiKey) {
    headers["X-AI-Api-Key"] = c.aiApiKey;
  } else {
    console.error(
      "AI 用のキー(SENN_AI_API_KEY)がありません。senn_connect_account でこの端末を接続してください(承認されると自動で使えるようになります)。"
    );
  }

  return { baseUrl: checked.baseUrl, headers };
}
