// @vitest-environment jsdom
/**
 * AiSettings.test.tsx — 設定 → AI の、モデルの実在の確認(DEMO-000146)
 *
 * 2026-10-10: 設定のモデルが 7b、Ollama には 3b しか無かったのに、画面は一覧の先頭(3b)を
 * 設定中の値のように表示していて、気づけなかった。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { I18nextProvider } from 'react-i18next';
import i18n from 'i18next';
import { AiSettings } from './AiSettings';
import { apiClient } from '@/shared/api/client';

vi.mock('@/shared/api/client');
vi.mock('@/shared/stores/toastStore', () => ({
  useToast: () => ({ success: vi.fn(), error: vi.fn(), info: vi.fn() }),
}));

if (!i18n.isInitialized) {
  void i18n.init({
    lng: 'ja',
    fallbackLng: 'ja',
    resources: {
      ja: {
        translation: {
          settings: {
            aiTitle: 'AI',
            aiOllamaStatus: 'Ollama 接続状態',
            aiModelLabel: 'SENN の AI 機能で使うモデル',
            aiEnvDefault: '環境変数の既定値: {{model}}',
            aiModelNotInstalled: 'Ollama に入っていません',
            aiModelMissing:
              '設定中のモデル「{{model}}」は、Ollama に入っていません。({{available}})',
          },
          ai: { connected: '接続済み', disconnected: '未接続' },
        },
      },
    },
    react: { useSuspense: false },
  });
}

const base = {
  ollamaModel: 'qwen2.5:3b',
  envDefaultModel: 'qwen2.5:3b',
  ollamaTimeoutSecs: 240,
  envDefaultTimeoutSecs: 240,
  ollamaConnected: true,
  availableModels: ['qwen2.5:3b'],
  ollamaModelInstalled: true as boolean | null,
  canEdit: true,
};

function renderWith(data: typeof base) {
  (apiClient.get as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({ data });
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <I18nextProvider i18n={i18n}>
        <AiSettings />
      </I18nextProvider>
    </QueryClientProvider>,
  );
}

describe('AiSettings: 設定のモデルが Ollama にあるか', () => {
  beforeEach(() => vi.clearAllMocks());
  afterEach(cleanup);

  it('モデルが Ollama にあるときは、警告を出さない', async () => {
    renderWith(base);
    expect(await screen.findByTestId('ai-settings')).toBeInTheDocument();
    expect(screen.queryByTestId('ai-model-missing')).not.toBeInTheDocument();
  });

  it('Ollama に無いモデルが設定されているとき、警告を出し、設定中の値を選択肢に出す(一覧の先頭を、設定中のように見せない)', async () => {
    renderWith({ ...base, ollamaModel: 'qwen2.5:7b', ollamaModelInstalled: false });
    const alert = await screen.findByTestId('ai-model-missing');
    expect(alert).toHaveAttribute('role', 'alert');
    expect(alert).toHaveTextContent('qwen2.5:7b');
    expect(alert).toHaveTextContent('qwen2.5:3b');
    const select = screen.getByRole('combobox') as HTMLSelectElement;
    expect(select.value).toBe('qwen2.5:7b');
    expect(screen.getByRole('option', { name: 'qwen2.5:7b (Ollama に入っていません)' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'qwen2.5:3b' })).toBeInTheDocument();
  });

  it('Ollama に接続できず判断できない(null)ときは、警告を出さない', async () => {
    renderWith({
      ...base,
      ollamaConnected: false,
      availableModels: [],
      ollamaModelInstalled: null,
    });
    expect(await screen.findByTestId('ai-settings')).toBeInTheDocument();
    expect(screen.queryByTestId('ai-model-missing')).not.toBeInTheDocument();
  });

  it('古い応答(ollamaModelInstalled が無い)でも、警告を出さず壊れない', async () => {
    const { ollamaModelInstalled: _omit, ...legacy } = base;
    void _omit;
    renderWith(legacy as typeof base);
    expect(await screen.findByTestId('ai-settings')).toBeInTheDocument();
    expect(screen.queryByTestId('ai-model-missing')).not.toBeInTheDocument();
  });

  it('編集できない人にも、警告は見える', async () => {
    renderWith({ ...base, ollamaModel: 'qwen2.5:7b', ollamaModelInstalled: false, canEdit: false });
    expect(await screen.findByTestId('ai-model-missing')).toBeInTheDocument();
  });
});
