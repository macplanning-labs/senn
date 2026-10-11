/**
 * 「完了」へ変える入口が、クローズ前の確認(useCloseGuard)を通っていることの検査。
 *
 * 経緯: 第1弾の stg で、確認を入れたのはサイドパネル(TicketDetailPanel)だけで、
 * 全画面の詳細ページ(TicketPropertiesSidebar の Status)は素通りしていた。
 * ステータスを変える画面を足したり直したりしたときに、同じ取りこぼしが起きないよう、
 * 入口になっているコンポーネントが確認を経由していることをソースで確かめる。
 * 新しい入口(別のコンポーネント)を足したら、ここの一覧にも足すこと。
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';

const ENTRY_POINTS = [
  'components/TicketPropertiesSidebar.tsx', // 全画面の詳細ページ(右側 PROPERTIES の Status)
  'components/TicketDetailPanel.tsx', // サイドパネル(ボード・自分のチケット・サイクル詳細)
];

function read(rel: string): string {
  return readFileSync(new URL(`./${rel}`, import.meta.url), 'utf-8');
}

describe('完了への変更は useCloseGuard を通る', () => {
  for (const rel of ENTRY_POINTS) {
    it(`${rel} は確認フックを使い、確認ダイアログを描画する`, () => {
      const src = read(rel);
      expect(src).toContain('useCloseGuard(');
      expect(src).toContain('closeGuard.request(');
      expect(src).toContain('<CloseCheckDialog');
    });
  }

  it('TicketPropertiesSidebar が、ステータスの変更を直接 mutate していない', () => {
    // 確認を通さない直接呼び出しは、フックのコールバック内の 1 箇所だけ
    const src = read('components/TicketPropertiesSidebar.tsx');
    const direct = src.match(/mutations\.status\.mutate\(/g) ?? [];
    expect(direct.length).toBe(1);
    expect(src).toMatch(/useCloseGuard\([^)]*\(status: string\) => mutations\.status\.mutate\(status\)/);
  });
});
