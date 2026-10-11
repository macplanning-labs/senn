/**
 * aiKickFormat.ts — AI 自動実行の設定値フォーマット・検証
 */

/**
 * トークン数を人間が読める形式に変換（1.2M、500K など）
 */
export function formatTokens(tokens: number | null | undefined): string {
  if (tokens === null || tokens === undefined) return '—';
  if (tokens === 0) return '0';

  const abs = Math.abs(tokens);
  if (abs >= 1_000_000) {
    return (tokens / 1_000_000).toFixed(1).replace(/\.?0+$/, '') + 'M';
  }
  if (abs >= 1_000) {
    return (tokens / 1_000).toFixed(1).replace(/\.?0+$/, '') + 'K';
  }
  return tokens.toString();
}

/**
 * 使用量の割合から、プログレスバーの CSS クラスを返す
 * 50%=黄、80%=橙、100% 以上=赤
 */
export function usageBarClass(percent: number | null | undefined): string {
  if (percent === null || percent === undefined) return 'ai-kick__usage-bar--default';
  if (percent >= 100) return 'ai-kick__usage-bar--error';
  if (percent >= 80) return 'ai-kick__usage-bar--warning';
  if (percent >= 50) return 'ai-kick__usage-bar--caution';
  return 'ai-kick__usage-bar--default';
}

/**
 * ランナー名の形式を検証
 * 英数字・ハイフン・アンダースコア・ピリオド、1〜80 字
 */
export function validateRunnerName(name: string): string | null {
  if (!name.trim()) {
    return 'ランナー名を入力してください';
  }
  if (name.length > 80) {
    return 'ランナー名は 80 字以内です';
  }
  if (!/^[A-Za-z0-9\-_.]+$/.test(name)) {
    return '英数字・ハイフン・アンダースコア・ピリオドのみ使用できます';
  }
  return null;
}

/**
 * リポジトリ形式（owner/name）を検証
 */
export function validateRepo(repo: string): string | null {
  if (!repo.trim()) {
    return 'リポジトリが必須です';
  }
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repo)) {
    return 'リポジトリは「owner/name」の形式です';
  }
  return null;
}

/**
 * 上限値の草稿を検証（ユーザー入力値）
 * @param draft 入力値（整数のみを想定。負の数・小数・非数値を排除）
 * @returns エラーメッセージ（複数の場合は改行で結合）
 */
export function validateLimitsDraft(draft: {
  maxTurns?: number | null;
  maxMinutes?: number | null;
  maxWeightedTokens?: number | null;
  dailyBudgetSystem?: number | null;
  dailyBudgetProject?: number | null;
  weeklyBudgetSystem?: number | null;
  perUserHour?: number | null;
  perUserDay?: number | null;
  perTicket24h?: number | null;
  maxQueue?: number | null;
  burnRatePerMin?: number | null;
  maxConsecutiveFailures?: number | null;
  hardCeilingPercent?: number | null;
  emergencyCapPercent?: number | null;
}): string[] {
  const errors: string[] = [];

  // 個別チェック：整数・非負
  const integerFields = [
    'maxTurns',
    'maxMinutes',
    'maxWeightedTokens',
    'dailyBudgetSystem',
    'dailyBudgetProject',
    'weeklyBudgetSystem',
    'perUserHour',
    'perUserDay',
    'perTicket24h',
    'maxQueue',
    'burnRatePerMin',
    'maxConsecutiveFailures',
    'hardCeilingPercent',
    'emergencyCapPercent',
  ] as const;

  for (const key of integerFields) {
    const val = draft[key];
    if (val !== null && val !== undefined) {
      if (!Number.isInteger(val)) {
        errors.push(`${key}: 整数を入力してください`);
      } else if (val < 0) {
        errors.push(`${key}: 0 以上の値を入力してください`);
      }
    }
  }

  // 関係チェック
  if (
    draft.dailyBudgetProject &&
    draft.dailyBudgetSystem &&
    draft.dailyBudgetProject > draft.dailyBudgetSystem
  ) {
    errors.push('プロジェクト日次の予算は、システム全体の日次を超えてはいけません');
  }

  if (
    draft.maxWeightedTokens &&
    draft.dailyBudgetSystem &&
    draft.maxWeightedTokens > draft.dailyBudgetSystem
  ) {
    errors.push('1 実行の最大トークン数は、システム全体の日次予算を超えてはいけません');
  }

  if (
    draft.maxWeightedTokens &&
    draft.dailyBudgetProject &&
    draft.maxWeightedTokens > draft.dailyBudgetProject
  ) {
    errors.push('1 実行の最大トークン数は、プロジェクト日次の予算を超えてはいけません');
  }

  if (
    draft.perUserDay &&
    draft.perUserHour &&
    draft.perUserDay < draft.perUserHour
  ) {
    errors.push('1 人あたり 1 日の上限は、1 時間の上限以上である必要があります');
  }

  if (draft.hardCeilingPercent !== null && draft.hardCeilingPercent !== undefined) {
    if (draft.hardCeilingPercent <= 100) {
      errors.push('上限の線は 100% より大きい値を設定してください');
    }
  }

  return errors;
}
