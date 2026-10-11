/**
 * GitHub App integration utilities
 */

export interface RelativeTime {
  value: number;
  unit: 'seconds' | 'minutes' | 'hours' | 'days' | 'months';
}

/**
 * Calculate relative time between ISO 8601 timestamp and now
 * Returns value and unit for formatting by caller/i18n
 */
export function calculateRelativeTime(isoString: string): RelativeTime {
  const date = new Date(isoString);
  const now = new Date();
  const secondsDiff = Math.floor((now.getTime() - date.getTime()) / 1000);

  if (secondsDiff < 60) {
    return { value: secondsDiff, unit: 'seconds' };
  }
  const minutesDiff = Math.floor(secondsDiff / 60);
  if (minutesDiff < 60) {
    return { value: minutesDiff, unit: 'minutes' };
  }
  const hoursDiff = Math.floor(minutesDiff / 60);
  if (hoursDiff < 24) {
    return { value: hoursDiff, unit: 'hours' };
  }
  const daysDiff = Math.floor(hoursDiff / 24);
  if (daysDiff < 30) {
    return { value: daysDiff, unit: 'days' };
  }
  const monthsDiff = Math.floor(daysDiff / 30);
  return { value: monthsDiff, unit: 'months' };
}

/**
 * Format relative time for display (language-agnostic)
 * Caller should use i18n for localization
 */
export function formatRelativeTime(isoString: string): string {
  const rel = calculateRelativeTime(isoString);
  const units: Record<string, string> = {
    seconds: 's',
    minutes: 'm',
    hours: 'h',
    days: 'd',
    months: 'mo',
  };
  return `${rel.value}${units[rel.unit] || ''}`;
}

/**
 * リポジトリ検索キーワードで絞り込み
 */
export function filterRepositories<T extends { fullName: string }>(
  repos: T[],
  query: string,
): T[] {
  if (!query.trim()) {
    return repos;
  }
  const lower = query.toLowerCase();
  return repos.filter((repo) => repo.fullName.toLowerCase().includes(lower));
}

/**
 * Webhook 署名比較用（暗号化関数からは分離）
 */
export function getDeliveryStatusColor(
  result: 'ok' | 'no_ticket' | 'ignored' | 'error' | null | undefined,
): 'success' | 'warning' | 'error' | 'neutral' {
  switch (result) {
    case 'ok':
      return 'success';
    case 'no_ticket':
    case 'ignored':
      return 'warning';
    case 'error':
      return 'error';
    default:
      return 'neutral';
  }
}

/**
 * App 作成用の Manifest フォーム送信
 * GitHub の仕様により、manifest は JSON を文字列化して hidden input で送信
 */
export function submitManifestForm(postUrl: string, manifest: Record<string, unknown>): void {
  const form = document.createElement('form');
  form.method = 'POST';
  form.action = postUrl;

  const input = document.createElement('input');
  input.type = 'hidden';
  input.name = 'manifest';
  input.value = JSON.stringify(manifest);

  form.appendChild(input);
  document.body.appendChild(form);
  form.submit();
}

/**
 * 連携を作る人が、そのチームのメンバーではないときに案内を出すか。
 * メンバーではないシステム管理者が作ると、操作の記録(自動でステータスが動いたときの担当)が管理者の名前になる。
 * サーバーが判定した値(viewerIsMember)だけで決める。false のときだけ true(null・未指定では出さない)。
 */
export function shouldWarnNotMember(status: { viewerIsMember?: boolean | null } | null | undefined): boolean {
  return status?.viewerIsMember === false;
}
