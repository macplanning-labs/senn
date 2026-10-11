// @vitest-environment jsdom
import { describe, it, expect, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { I18nextProvider } from 'react-i18next';
import i18n from 'i18next';
import { SimilarIncidentsNeedProjectHint, SimilarIncidentsPanel } from './SimilarIncidentsPanel';
import type { SimilarIncident } from '@/shared/api/types';

if (!i18n.isInitialized) {
  void i18n.init({
    lng: 'en',
    fallbackLng: 'en',
    resources: {
      en: {
        translation: {
          similarIncidents: {
            title: 'Similar past incidents',
            hint: 'Reference only.',
            showDetail: 'Show detail',
            symptom: 'Symptom',
            cause: 'Cause',
            solution: 'Fix',
            writeDescriptionHint: 'Write a description too.',
            selectProjectHint: 'Choose a project.',
          },
        },
      },
    },
  });
}

const inc = (over: Partial<SimilarIncident> = {}): SimilarIncident => ({
  ticketKey: 'DEMO-000001',
  title: '請求書の出力が失敗する',
  score: 2,
  reasons: ['共通する語: 請求書、一括出力'],
  symptom: '落ちる',
  cause: '待ちが短い',
  solution: '延ばした',
  ...over,
});

const renderPanel = (
  incidents: SimilarIncident[],
  over: { searched?: boolean; descriptionEmpty?: boolean } = {},
) =>
  render(
    <I18nextProvider i18n={i18n}>
      <SimilarIncidentsPanel
        projectPrefix="DEMO"
        incidents={incidents}
        searched={over.searched ?? true}
        descriptionEmpty={over.descriptionEmpty ?? false}
      />
    </I18nextProvider>,
  );

describe('SimilarIncidentsPanel', () => {
  afterEach(cleanup);

  it('0 件で説明が書かれていれば、何も出さない', () => {
    const { container } = renderPanel([], { descriptionEmpty: false });
    expect(container).toBeEmptyDOMElement();
  });

  it('0 件で説明が空なら、「説明も書くと見つかりやすい」と短く案内する', () => {
    renderPanel([], { descriptionEmpty: true });
    expect(screen.getByTestId('similar-incidents-hint')).toHaveTextContent('Write a description too.');
    expect(screen.queryByTestId('similar-incidents')).not.toBeInTheDocument();
  });

  it('検索できていない(off・失敗・入力待ち)ときは、案内も含めて何も出さない', () => {
    const { container } = renderPanel([], { searched: false, descriptionEmpty: true });
    expect(container).toBeEmptyDOMElement();
  });

  it('見つかったときも、説明が空なら案内を添える。書かれていれば添えない', () => {
    renderPanel([inc()], { descriptionEmpty: true });
    expect(screen.getByTestId('similar-incident')).toBeInTheDocument();
    expect(screen.getByTestId('similar-incidents-hint')).toBeInTheDocument();
    cleanup();
    renderPanel([inc()], { descriptionEmpty: false });
    expect(screen.queryByTestId('similar-incidents-hint')).not.toBeInTheDocument();
  });

  it('キー・題名・理由・症状・原因・対策を出し、リンクは新しいタブで開く', () => {
    renderPanel([inc()]);
    expect(screen.getByTestId('similar-incidents')).toBeInTheDocument();
    const link = screen.getByRole('link', { name: 'DEMO-000001' });
    expect(link).toHaveAttribute('href', '/project/DEMO/tickets/DEMO-000001');
    expect(link).toHaveAttribute('target', '_blank');
    expect(link).toHaveAttribute('rel', expect.stringContaining('noopener'));
    expect(screen.getByText('請求書の出力が失敗する')).toBeInTheDocument();
    expect(screen.getByText('共通する語: 請求書、一括出力')).toBeInTheDocument();
    expect(screen.getByText('Symptom: 落ちる')).toBeInTheDocument();
    expect(screen.getByText('Cause: 待ちが短い')).toBeInTheDocument();
    expect(screen.getByText('Fix: 延ばした')).toBeInTheDocument();
    // 症状・原因・対策は、既定で折り畳まれている(押すと開く)
    const detail = screen.getByTestId('similar-incident-detail') as HTMLDetailsElement;
    expect(detail.open).toBe(false);
    expect(screen.getByText('Show detail')).toBeInTheDocument();
  });

  it('無い項目は出さない', () => {
    renderPanel([inc({ symptom: null, cause: null, solution: null, reasons: [] })]);
    expect(screen.queryByText(/Symptom/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Cause/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Fix/)).not.toBeInTheDocument();
  });

  it('HTML は解釈せず、文字として表示する', () => {
    const { container } = renderPanel([inc({ title: '<img src=x onerror=alert(1)>' })]);
    expect(container.querySelector('img')).toBeNull();
    expect(screen.getByText('<img src=x onerror=alert(1)>')).toBeInTheDocument();
  });

  it('プロジェクト未選択のときは、選ぶと探せることを案内する', () => {
    render(
      <I18nextProvider i18n={i18n}>
        <SimilarIncidentsNeedProjectHint />
      </I18nextProvider>,
    );
    expect(screen.getByTestId('similar-incidents-need-project')).toHaveTextContent('Choose a project.');
  });
});
