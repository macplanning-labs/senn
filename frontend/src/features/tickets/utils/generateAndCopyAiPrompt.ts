/**
 * generateAndCopyAiPrompt.ts - AI プロンプト生成・コピーユーティリティ
 *
 * プロンプトが未生成の場合、その場で生成してクリップボードにコピーする
 */

import { apiClient } from '@/shared/api/client';
import { buildCopyPrompt } from './buildCopyPrompt';
import type { UserAiPromptTemplates } from './buildCopyPrompt';

/**
 * AI プロンプト生成 API のレスポンス型
 */
interface GenerateAiPromptResponse {
  aiPrompt: string | null;
}

/**
 * AI プロンプト生成・コピーユーティリティ
 *
 * @param ticketKey - チケットキー（例: "PROJ-123"）
 * @param isGenerating - 生成中フラグ (Ref<boolean> または state)
 * @param codingTool - 選択中のツール
 * @param userTemplates - ユーザーの個人設定テンプレート
 * @param defaults - 既定値テンプレート
 * @param getI18nString - 翻訳関数
 * @returns 成否と、表示すべきトースト
 */
export async function generateAndCopyAiPrompt(
  ticketKey: string,
  isGenerating: { current: boolean } | { value: boolean },
  codingTool: 'Cursor' | 'Claude Code',
  userTemplates: UserAiPromptTemplates | undefined,
  defaults: { common: string; cursor: string; claude: string },
  getI18nString: (key: string) => string,
): Promise<{
  success: boolean;
  toast: { message: string; type: 'success' | 'info' | 'error' };
}> {
  // 生成中フラグをチェック
  const isGen = 'current' in isGenerating ? isGenerating.current : isGenerating.value;
  if (isGen) {
    return {
      success: false,
      toast: { message: getI18nString('ticketDetail.aiPromptGenerating'), type: 'info' },
    };
  }

  // フラグをセット
  if ('current' in isGenerating) {
    isGenerating.current = true;
  } else {
    isGenerating.value = true;
  }

  try {
    // プロンプト生成API呼び出し
    const response = await apiClient.post<GenerateAiPromptResponse>(
      `/tickets/${ticketKey}/ai-prompt/`,
      undefined,
      // ローカルAI(Ollama)の要約に時間がかかるため、既定の30秒より長く待つ
      { timeout: 120_000 },
    );

    const aiPrompt = response.data.aiPrompt;
    if (!aiPrompt) {
      // null が返された場合は、簡易プロンプトをコピー
      return {
        success: false,
        toast: {
          message: getI18nString('ticketDetail.aiPromptPending'),
          type: 'info',
        },
      };
    }

    // プロンプトを個人設定のおまじないと結合
    const finalPrompt = buildCopyPrompt(aiPrompt, codingTool, userTemplates, defaults);

    // クリップボードに書き込み
    try {
      await navigator.clipboard.writeText(finalPrompt);
      return {
        success: true,
        toast: {
          message: getI18nString('ticketDetail.aiPromptCopied').replace(
            '{{ticketKey}}',
            ticketKey,
          ),
          type: 'success',
        },
      };
    } catch {
      // Safari 等でユーザー操作扱いが切れた場合
      return {
        success: false,
        toast: {
          message: getI18nString('ticketDetail.aiPromptCopyFailed'),
          type: 'error',
        },
      };
    }
  } catch (error) {
    // API呼び出し失敗
    console.error('Failed to generate AI prompt:', error);
    return {
      success: false,
      toast: {
        message: getI18nString('ticketDetail.aiPromptPending'),
        type: 'info',
      },
    };
  } finally {
    // フラグをリセット
    if ('current' in isGenerating) {
      isGenerating.current = false;
    } else {
      isGenerating.value = false;
    }
  }
}
