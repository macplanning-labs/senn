import {
  findPlaceholderCredentials,
  placeholderCredentialsMessage,
} from "../credentials-guard.js";
import { BASE_URL_MISSING_MESSAGE, checkBaseUrl, collectCredentials } from "../credentials-file.js";

export interface CliConfig {
  baseUrl: string;
  headers: Record<string, string>;
}

export function loadCliConfig(): CliConfig {
  // ローカル実行では credentials.env も読む。
  // クラウド実行(CLAUDE_CODE_REMOTE_SESSION_ID がある)では、プロキシが認証ヘッダを付けるので、ファイルは読まない
  const c = collectCredentials({ readFile: !process.env.CLAUDE_CODE_REMOTE_SESSION_ID });

  // テスト用の値で上書きされていたら止める
  const placeholders = findPlaceholderCredentials({
    SENN_CF_ACCESS_CLIENT_ID: c.cfAccessClientId,
    SENN_CF_ACCESS_CLIENT_SECRET: c.cfAccessClientSecret,
    SENN_AI_API_KEY: c.aiApiKey,
  });
  if (placeholders.length > 0) {
    throw new Error(
      placeholderCredentialsMessage(
        placeholders,
        "環境変数または ~/.config/senn-ai-agent/credentials.env"
      )
    );
  }

  if (!c.baseUrl) throw new Error(BASE_URL_MISSING_MESSAGE);
  const { baseUrl, warning } = checkBaseUrl(c.baseUrl);
  if (warning) console.error(warning);

  // ヘッダーを構築（設定されていない場合は付けない）
  const headers: Record<string, string> = {};
  if (c.cfAccessClientId) {
    headers["CF-Access-Client-Id"] = c.cfAccessClientId;
  }
  if (c.cfAccessClientSecret) {
    headers["CF-Access-Client-Secret"] = c.cfAccessClientSecret;
  }
  if (c.aiApiKey) {
    headers["X-AI-Api-Key"] = c.aiApiKey;
  }

  return { baseUrl, headers };
}
