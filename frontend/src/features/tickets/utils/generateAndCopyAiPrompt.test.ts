/**
 * generateAndCopyAiPrompt.test.ts - AI プロンプト生成・コピー ユーティリティテスト
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { generateAndCopyAiPrompt } from './generateAndCopyAiPrompt';
import * as client from '@/shared/api/client';

// Mock apiClient
vi.mock('@/shared/api/client', () => ({
  apiClient: {
    post: vi.fn(),
  },
}));

describe('generateAndCopyAiPrompt', () => {
  let clipboardWriteSpy: ReturnType<typeof vi.fn>;
  let isGeneratingRef: { current: boolean };
  const defaults = {
    common: 'default common',
    cursor: 'default cursor',
    claude: 'default claude',
  };
  const getI18nString = (key: string) => {
    const translations: Record<string, string> = {
      'ticketDetail.aiPromptGenerating': 'Generating prompt. Please wait…',
      'ticketDetail.aiPromptPending': 'Prompt not ready yet.',
      'ticketDetail.aiPromptCopied': 'Copied prompt for {{ticketKey}}',
      'ticketDetail.aiPromptCopyFailed': 'Failed to copy to clipboard',
      'ticketDetail.aiRunBlockSkipped': 'Copied prompt but AI work-record block is missing',
    };
    return translations[key] || key;
  };

  beforeEach(() => {
    isGeneratingRef = { current: false };
    clipboardWriteSpy = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: {
        writeText: clipboardWriteSpy,
      },
    });
    vi.clearAllMocks();
  });

  // ヘルパー：apiClient.post を URL で条件分岐するモック設定
  const setupMockPost = (mockFn: any) => {
    mockFn.mockImplementation((url: string, _data?: any, _opts?: any) => {
      if (url.includes('/ai-prompt/')) {
        // ai-prompt エンドポイント
        return Promise.resolve({ data: { aiPrompt: 'Generated prompt content' } });
      } else if (url.includes('/ai-run/')) {
        // ai-run エンドポイント
        return Promise.resolve({
          data: {
            runUuid: 'test-uuid-1234',
            instructionBlock: '--- AI Work Record Block ---\nInstructions here',
            reused: false,
          },
        });
      }
      return Promise.resolve({ data: {} });
    });
  };

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should return success when prompt is generated and copied with instruction block', async () => {
    const mockApiClient = client.apiClient as any;
    setupMockPost(mockApiClient.post);

    const result = await generateAndCopyAiPrompt(
      'PROJ-123',
      isGeneratingRef,
      'Cursor',
      undefined,
      defaults,
      getI18nString,
    );

    expect(result.success).toBe(true);
    expect(result.toast.type).toBe('success');
    expect(result.toast.message).toContain('PROJ-123');
    // クリップボードに「プロンプト + おまじない + 手順ブロック」が入ることを確認
    expect(clipboardWriteSpy).toHaveBeenCalledWith(
      'Generated prompt content\n\n---\ndefault common\ndefault cursor\n\n--- AI Work Record Block ---\nInstructions here',
    );
  });

  it('should return info toast when aiPrompt is null', async () => {
    const mockApiClient = client.apiClient as any;
    mockApiClient.post.mockResolvedValue({
      data: { aiPrompt: null },
    });

    const result = await generateAndCopyAiPrompt(
      'PROJ-456',
      isGeneratingRef,
      'Claude Code',
      undefined,
      defaults,
      getI18nString,
    );

    expect(result.success).toBe(false);
    expect(result.toast.type).toBe('info');
    expect(result.toast.message).toContain('not ready');
    // プロンプトが無いときは、AI実行の記録(run の発行)をしない=見張りの通知が増えない
    const urls = mockApiClient.post.mock.calls.map((c: unknown[]) => String(c[0]));
    expect(urls.some((u: string) => u.includes('/ai-run/'))).toBe(false);
    expect(clipboardWriteSpy).not.toHaveBeenCalled();
  });

  it('should return error when clipboard write fails', async () => {
    const mockApiClient = client.apiClient as any;
    mockApiClient.post.mockResolvedValue({
      data: { aiPrompt: 'Generated prompt' },
    });
    clipboardWriteSpy.mockRejectedValueOnce(new Error('Clipboard error'));

    const result = await generateAndCopyAiPrompt(
      'PROJ-789',
      isGeneratingRef,
      'Cursor',
      undefined,
      defaults,
      getI18nString,
    );

    expect(result.success).toBe(false);
    expect(result.toast.type).toBe('error');
    expect(result.toast.message).toContain('Failed to copy');
  });

  it('should handle API error and return info toast', async () => {
    const mockApiClient = client.apiClient as any;
    mockApiClient.post.mockRejectedValueOnce(new Error('API Error'));

    const result = await generateAndCopyAiPrompt(
      'PROJ-999',
      isGeneratingRef,
      'Cursor',
      undefined,
      defaults,
      getI18nString,
    );

    expect(result.success).toBe(false);
    expect(result.toast.type).toBe('info');
  });

  it('should prevent double submission with isGenerating flag', async () => {
    const mockApiClient = client.apiClient as any;
    isGeneratingRef.current = true;

    const result = await generateAndCopyAiPrompt(
      'PROJ-111',
      isGeneratingRef,
      'Cursor',
      undefined,
      defaults,
      getI18nString,
    );

    expect(result.success).toBe(false);
    expect(result.toast.message).toContain('Generating prompt');
    expect(mockApiClient.post).not.toHaveBeenCalled();
  });

  it('should reset isGenerating flag after completion', async () => {
    const mockApiClient = client.apiClient as any;
    mockApiClient.post.mockResolvedValue({
      data: { aiPrompt: 'Generated prompt' },
    });

    expect(isGeneratingRef.current).toBe(false);

    await generateAndCopyAiPrompt(
      'PROJ-222',
      isGeneratingRef,
      'Cursor',
      undefined,
      defaults,
      getI18nString,
    );

    expect(isGeneratingRef.current).toBe(false);
  });

  it('should handle empty template gracefully', async () => {
    const mockApiClient = client.apiClient as any;
    setupMockPost(mockApiClient.post);

    const result = await generateAndCopyAiPrompt(
      'PROJ-333',
      isGeneratingRef,
      'Cursor',
      { common: null, cursor: null, claude: null },
      { common: '', cursor: '', claude: '' },
      getI18nString,
    );

    expect(result.success).toBe(true);
    // 空のテンプレートでも instruction block は付く
    expect(clipboardWriteSpy).toHaveBeenCalledWith(
      'Generated prompt content\n\n--- AI Work Record Block ---\nInstructions here',
    );
  });

  it('should copy prompt and show info toast when ai-run fails', async () => {
    const mockApiClient = client.apiClient as any;
    mockApiClient.post.mockImplementation((url: string) => {
      if (url.includes('/ai-prompt/')) {
        return Promise.resolve({ data: { aiPrompt: 'Generated prompt' } });
      } else if (url.includes('/ai-run/')) {
        // ai-run が失敗
        return Promise.reject(new Error('AI run API failed'));
      }
      return Promise.resolve({ data: {} });
    });

    const result = await generateAndCopyAiPrompt(
      'PROJ-444',
      isGeneratingRef,
      'Claude Code',
      undefined,
      defaults,
      getI18nString,
    );

    expect(result.success).toBe(true);
    expect(result.toast.type).toBe('info');
    expect(result.toast.message).toContain('work-record block is missing');
    // プロンプト自体のコピーは成功（ai-run がないので手順ブロックは付かない）
    expect(clipboardWriteSpy).toHaveBeenCalledWith(
      'Generated prompt\n\n---\ndefault common\ndefault claude',
    );
  });

  it('should pass correct tool parameter to ai-run API', async () => {
    const mockApiClient = client.apiClient as any;
    setupMockPost(mockApiClient.post);

    await generateAndCopyAiPrompt(
      'PROJ-555',
      isGeneratingRef,
      'Claude Code',
      undefined,
      defaults,
      getI18nString,
    );

    // ai-run endpoint に claude_code ツールで呼ばれること
    expect(mockApiClient.post).toHaveBeenCalledWith(
      expect.stringContaining('/ai-run/'),
      expect.objectContaining({ tool: 'claude_code' }),
      expect.any(Object),
    );
  });

  it('should pass cursor tool parameter to ai-run API when Cursor is selected', async () => {
    const mockApiClient = client.apiClient as any;
    setupMockPost(mockApiClient.post);

    await generateAndCopyAiPrompt(
      'PROJ-666',
      isGeneratingRef,
      'Cursor',
      undefined,
      defaults,
      getI18nString,
    );

    // ai-run endpoint に cursor ツールで呼ばれること
    expect(mockApiClient.post).toHaveBeenCalledWith(
      expect.stringContaining('/ai-run/'),
      expect.objectContaining({ tool: 'cursor' }),
      expect.any(Object),
    );
  });
});
