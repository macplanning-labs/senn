/**
 * integrationQueryKeys.ts — 連携の一覧と、リポジトリ一覧(GitHub App)のキャッシュのキー
 *
 * 連携を削除すると、「連携の一覧」だけでなく「リポジトリ一覧の『連携済み』の印」も変わる。
 * 片方だけ取り直すと、削除したのに印が残る(DEMO-000182)。両方まとめて扱うために一か所に置く。
 */
export function integrationQueryKeys(scopeKey: string): string[][] {
  return [
    ['integrations', scopeKey],
    ['github-app', 'repositories', scopeKey],
  ];
}
