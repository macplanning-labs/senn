import { describe, it, expect, vi } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import i18n from '@/i18n';

vi.mock('./BackLink.css', () => ({}));

import { BackLink } from './BackLink';

function render(props: Parameters<typeof BackLink>[0]): string {
  return renderToStaticMarkup(createElement(MemoryRouter, null, createElement(BackLink, props)));
}

describe('BackLink(見た目・動作の共通ルール)', () => {
  it('「← 戻り先の一覧名」の形で、固定の親一覧へのリンクになる(履歴ではない)', () => {
    const html = render({ to: '/projects', label: 'プロジェクト一覧', testId: 'x' });
    expect(html).toContain('href="/projects"');
    expect(html).toContain('data-testid="x"');
    expect(html).toContain('プロジェクト一覧');
    // 矢印は装飾(読み上げない)
    expect(html).toContain('<span aria-hidden="true">← </span>');
  });

  it('横並びのヘッダーの中に置くときは、余白なしの inline 版', () => {
    expect(render({ to: '/a', label: 'A', inline: true })).toContain('back-link back-link--inline');
    expect(render({ to: '/a', label: 'A' })).not.toContain('back-link--inline');
  });

  it('戻り先の名前は、日本語・英語の両方で用意されている', () => {
    for (const key of ['projects', 'roadmaps', 'teams', 'tickets', 'cycles']) {
      for (const lng of ['ja', 'en']) {
        const path = `nav.backTo.${key}`;
        expect(i18n.t(path, { lng }), `${lng}:${path}`).not.toBe(path);
      }
    }
    expect(i18n.t('nav.backTo.projects', { lng: 'ja' })).toBe('プロジェクト一覧');
  });
});

// ── 統一のガード: 詳細画面が、独自の戻り方を作らず、共通の BackLink を使い続けること ──
// ソースを文字列として読み込む(Node の型を足さず、Vite の機能で読む)
const sources = import.meta.glob('/src/features/**/*.tsx', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

function source(path: string): string {
  const src = sources[path];
  if (src === undefined) throw new Error(`${path} が見つかりません`);
  return src;
}

describe('戻る操作の統一(ガード)', () => {
  // チーム詳細・チーム設定は対象外(2026-09-24): サイドバーから直接開く画面のため「← チーム一覧」を置かない。
  // チーム一覧へはサイドバーのチーム見出しの「…」から移る。直前の画面へはヘッダーの「戻る / 進む」で戻る
  const screens: Record<string, string> = {
    'プロジェクト詳細': '/src/features/projects/components/ProjectLayout.tsx',
    'ロードマップ一覧': '/src/features/projects/components/RoadmapsList.tsx',
    'ロードマップ詳細': '/src/features/projects/components/RoadmapDetail.tsx',
    'チケット詳細': '/src/features/tickets/components/TicketDetail.tsx',
    'サイクル詳細': '/src/features/cycles/components/CycleDetail.tsx',
  };

  for (const [name, file] of Object.entries(screens)) {
    it(`${name}は、共通の BackLink を使う(独自の「← …」を書かない)`, () => {
      const src = source(file);
      expect(src).toContain("from '@/shared/components/ui/BackLink'");
      expect(src).toContain('<BackLink');
      // 画面に直接書いた「← 」や、履歴で戻る「戻る」ボタンを持たない
      expect(src).not.toMatch(/>\s*←\s/);
      expect(src).not.toContain('← 戻る');
    });
  }

  it('画面ごとの専用の戻るリンク(className に *__back)を、新しく作らない', () => {
    // (テスト環境では CSS ファイルを読み込むと空になるため、クラス名を書く TSX を調べる)
    // 認証画面の「ログインへ戻る」(__back-link)と、モーダル内の手順の「戻る」(__back-btn)は、
    // 詳細画面の戻りではないので、-link / -btn 付きは対象外
    const offenders = Object.entries(sources)
      .filter(([path, src]) => !path.endsWith('.test.tsx') && /__back(?![-\w])/.test(src))
      .map(([path]) => path);
    expect(offenders).toEqual([]);
  });

  it('このガード自身が、ソースを実際に読めている(空文字を調べて素通りしていない)', () => {
    const files = Object.keys(sources);
    expect(files.length).toBeGreaterThan(50);
    expect(source('/src/features/projects/components/ProjectLayout.tsx').length).toBeGreaterThan(500);
  });
});
