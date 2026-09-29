/**
 * generateAndCopyAiPrompt.test.ts - AI プロンプト生成・コピー ユーティリティテスト
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { generateAndCopyAiPrompt } from './generateAndCopyAiPrompt';
import * as client from '@/shared/api/client';
import type { UserAiPromptTemplates } from './buildCopyPrompt';

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

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should return success when prompt is generated and copied', async () => {
    const mockApiClient = client.apiClient as any;
    mockApiClient.post.mockResolvedValue({
      data: { aiPrompt: 'Generated prompt content' },
    });

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
    expect(clipboardWriteSpy).toHaveBeenCalledWith(
      'Generated prompt content\n\n---\ndefault common\ndefault cursor',
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
    mockApiClient.post.mockResolvedValue({
      data: { aiPrompt: 'Generated prompt' },
    });

    const result = await generateAndCopyAiPrompt(
      'PROJ-333',
      isGeneratingRef,
      'Cursor',
      { common: null, cursor: null, claude: null },
      { common: '', cursor: '', claude: '' },
      getI18nString,
    );

    expect(result.success).toBe(true);
    expect(clipboardWriteSpy).toHaveBeenCalledWith('Generated prompt');
  });
});
