/**
 * Cloudflare Access に止められた応答かどうかを見分ける。
 *
 * SENN の前に Cloudflare Access を置き、/api/v1/ai-agent/ をサービストークンで守っている場合、
 * トークンが違うと SENN 本体に届く前に Cloudflare が HTML の 403 を返す。
 * SENN 本体のエラーは JSON なので、HTML の中身で区別できる。Cloudflare を使わない環境では関係しない。
 */

export function isCloudflareAccessBlock(
  bodyText: string,
  headers?: { get(name: string): string | null }
): boolean {
  if (headers?.get("cf-access-domain")) return true;
  return /Cloudflare Access|cloudflareaccess\.com/i.test(bodyText);
}

export function cloudflareAccessBlockMessage(status: number): string {
  return (
    `Cloudflare Access に止められました（HTTP ${status}）。SENN 本体には届いていません。` +
    `サービストークン（SENN_CF_ACCESS_CLIENT_ID / SENN_CF_ACCESS_CLIENT_SECRET）が違う・空・期限切れの可能性があります。`
  );
}
