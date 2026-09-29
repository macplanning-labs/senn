/**
 * buildCopyPrompt.ts - ツール別のAIプロンプト生成ユーティリティ
 *
 * キャッシュ済みプロンプトの末尾に個人設定のおまじないを結合する
 */

export interface UserAiPromptTemplates {
  common: string | null;
  cursor: string | null;
  claude: string | null;
}

/**
 * キャッシュ済みプロンプトに個人設定のおまじないを結合する
 *
 * @param cachedPrompt - サーバーから取得したキャッシュ済みプロンプト
 * @param codingTool - 選択中のツール('Cursor' または 'Claude Code')
 * @param userTemplates - ユーザーの個人設定テンプレート
 * @param defaults - 既定値テンプレート
 * @returns 結合されたプロンプト
 */
export function buildCopyPrompt(
  cachedPrompt: string | null | undefined,
  codingTool: 'Cursor' | 'Claude Code',
  userTemplates: UserAiPromptTemplates | undefined,
  defaults: { common: string; cursor: string; claude: string },
): string {
  if (!cachedPrompt?.trim()) {
    return '';
  }

  const prompt = cachedPrompt.trim();

  // 共通テンプレートと選択中ツール用テンプレートを決定
  const commonTemplate = userTemplates?.common ?? defaults.common;
  const toolSpecificTemplate = codingTool === 'Cursor'
    ? (userTemplates?.cursor ?? defaults.cursor)
    : (userTemplates?.claude ?? defaults.claude);

  // 空の部分を除いて結合
  const parts: string[] = [];

  if (commonTemplate?.trim()) {
    parts.push(commonTemplate.trim());
  }

  if (toolSpecificTemplate?.trim()) {
    parts.push(toolSpecificTemplate.trim());
  }

  if (parts.length === 0) {
    return prompt;
  }

  return `${prompt}\n\n---\n${parts.join('\n')}`;
}
