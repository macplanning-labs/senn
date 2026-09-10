/**
 * ローカルタイムゾーン基準の YYYY-MM-DD 文字列。
 * toISOString().slice(0, 10) は UTC 日付になるため、JST 等で日付境界がずれる。
 * バックエンドの CURRENT_DATE 判定と揃えるためローカル日付を使う。
 */
export function localDateStr(offsetDays = 0): string {
  const d = new Date();
  d.setDate(d.getDate() + offsetDays);
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}
