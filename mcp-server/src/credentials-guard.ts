/**
 * 認証情報がテスト用の値で上書きされていないかを確かめる。
 *
 * テストが本物の credentials.env を fake-client-id などで上書きすると、再起動したときに
 * 403 だけが返り、原因が分からなくなる。起動時に「テスト用の値」を見つけて止める。
 *
 * 値が空のもの（未設定）はここでは問わない。Cloudflare を使わない環境（OSS 版など）では
 * CF_ACCESS_* が無いのが正しいため。必須かどうかは呼び出し側が決める。
 *
 * run-with-credentials.sh にも同じ判定がある。直すときは両方そろえる。
 */

/** テスト・説明用によく使われる値の先頭。本物の鍵（16進数や英数字の乱数）とは重ならない。 */
const PLACEHOLDER_PREFIX =
  /^(fake|test|dummy|example|sample|placeholder|changeme|change-me|your|xxx)([-_.]|$)/i;

/** 説明文の書き方（<...> や ... ）のまま入っている値。 */
const PLACEHOLDER_SHAPE = /^(<.*>|\.\.\.|\*+)$/;

export function isPlaceholderCredential(value: string): boolean {
  const v = value.trim();
  return PLACEHOLDER_PREFIX.test(v) || PLACEHOLDER_SHAPE.test(v);
}

/**
 * テスト用に見える値が入っている変数の名前を返す（値そのものは返さない）。
 * 空・未設定の変数は対象外。
 */
export function findPlaceholderCredentials(
  values: Record<string, string | undefined>
): string[] {
  return Object.entries(values)
    .filter(([, v]) => v !== undefined && v.trim() !== "" && isPlaceholderCredential(v))
    .map(([k]) => k);
}

/** 利用者向けのエラー文。秘密の値は含めない。 */
export function placeholderCredentialsMessage(names: string[], source: string): string {
  return (
    `認証情報がテスト用の値になっています: ${names.join(", ")}（読み込み元: ${source}）。` +
    `テストで credentials.env が上書きされた可能性があります。` +
    `本物の値に戻してから起動し直してください。`
  );
}
