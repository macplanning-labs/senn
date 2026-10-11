/**
 * gitRules.ts — Git ステータスルールのユーティリティ関数
 *
 * ルール形式の変換・検証・表示用ラベル生成
 */

import type { GitStatusRule, GitStatusRuleStatus } from '@/shared/api/types';

export type RuleValue = 'default' | 'none' | `category:${string}` | `slug:${string}`;

export interface RuleForm {
  trigger: string;
  value: RuleValue;
}

export interface RuleToSend {
  trigger: string;
  action: 'set' | 'none';
  targetSlug?: string;
  targetCategory?: string;
}

/** ルール形式を「表示用フォーム」に変換 */
export function ruleToForm(rule: GitStatusRule): RuleForm {
  if (rule.isDefault) {
    return { trigger: rule.trigger, value: 'default' };
  }
  if (rule.action === 'none') {
    return { trigger: rule.trigger, value: 'none' };
  }
  if (rule.targetCategory) {
    return { trigger: rule.trigger, value: `category:${rule.targetCategory}` };
  }
  if (rule.targetSlug) {
    return { trigger: rule.trigger, value: `slug:${rule.targetSlug}` };
  }
  // fallback
  return { trigger: rule.trigger, value: 'default' };
}

/** 「表示用フォーム」をルール形式に変換(保存用) */
export function formToRule(form: RuleForm): RuleToSend | null {
  const { trigger, value } = form;

  if (value === 'default') {
    // 標準は送らない(空のrules配列で標準に戻す)
    return null;
  }

  if (value === 'none') {
    return {
      trigger,
      action: 'none',
    };
  }

  if (value.startsWith('category:')) {
    const category = value.slice(9);
    return {
      trigger,
      action: 'set',
      targetCategory: category,
    };
  }

  if (value.startsWith('slug:')) {
    const slug = value.slice(5);
    return {
      trigger,
      action: 'set',
      targetSlug: slug,
    };
  }

  return null;
}

/** 変更があったルール(デフォルト以外)だけを集める */
export function collectChangedRules(rules: RuleForm[]): RuleToSend[] {
  return rules
    .map(formToRule)
    .filter((rule): rule is RuleToSend => rule !== null);
}

/** セレクトの選択肢を生成 */
export function generateSelectOptions(
  statuses: GitStatusRuleStatus[],
): Array<{ label: string; value: RuleValue }> {
  // NOTE: このオプションリストは、React コンポーネントから t() を使って翻訳される
  // ここでは翻訳キーを保持し、実際の翻訳はコンポーネントで行う
  const options: Array<{ label: string; value: RuleValue }> = [
    { label: 'integration.gitRules.default', value: 'default' },
    { label: 'integration.gitRules.none', value: 'none' },
  ];

  // カテゴリグループ
  const categories = [
    'backlog',
    'unstarted',
    'started',
    'review',
    'completed',
    'cancelled',
  ];

  for (const cat of categories) {
    options.push({
      label: cat,
      value: `category:${cat}` as RuleValue,
    });
  }

  // ステータスグループ
  for (const status of statuses) {
    options.push({
      label: status.name,
      value: `slug:${status.slug}` as RuleValue,
    });
  }

  return options;
}

/** セレクトをoptgroupで整理する */
export interface OptionGroup {
  label?: string;
  options: Array<{ label: string; value: RuleValue; i18nKey?: string }>;
}

export function generateOptionGroups(
  statuses: GitStatusRuleStatus[],
): OptionGroup[] {
  const groups: OptionGroup[] = [];

  // 基本選択肢
  groups.push({
    options: [
      { label: 'default', value: 'default', i18nKey: 'integration.gitRules.default' },
      { label: 'none', value: 'none', i18nKey: 'integration.gitRules.none' },
    ],
  });

  // カテゴリグループ
  const categories = [
    'backlog',
    'unstarted',
    'started',
    'review',
    'completed',
    'cancelled',
  ];

  groups.push({
    label: 'integration.gitRules.categoryLabel',
    options: categories.map((cat) => ({
      label: cat,
      value: `category:${cat}` as RuleValue,
      i18nKey: `settings.categoryLabels.${cat}`,
    })),
  });

  // ステータスグループ
  groups.push({
    label: 'integration.gitRules.statusLabel',
    options: statuses.map((status) => ({
      label: status.name,
      value: `slug:${status.slug}` as RuleValue,
    })),
  });

  return groups;
}

/** カテゴリの翻訳キー(i18n) */
export function categoryI18nKey(category: string): string {
  return `settings.categories.${category}`;
}

/** 標準の動作を表示用オブジェクト(i18n キー付き)を返す */
export interface BuiltinLabel {
  text: string;
  i18nKey?: string;
}

export function builtinDisplayLabel(
  rule: GitStatusRule,
  statuses: GitStatusRuleStatus[],
): BuiltinLabel | null {
  if (!rule.isDefault) {
    return null;
  }

  if (rule.action === 'none') {
    return {
      text: 'none',
      i18nKey: 'integration.gitRules.none',
    };
  }

  if (rule.targetCategory) {
    return {
      text: rule.targetCategory,
      i18nKey: `settings.categoryLabels.${rule.targetCategory}`,
    };
  }

  if (rule.targetSlug) {
    const status = statuses.find((s) => s.slug === rule.targetSlug);
    if (status) {
      return {
        text: status.name,
      };
    }
  }

  return null;
}

/**
 * 標準の動作(バックエンドの `builtin_rule` と同じ内容。何も設定しないときの動作)。
 * 「標準」を選んでいる行に、標準が何をするのかを示すために使う。
 * 保存済みの規則があるトリガーでも、「標準に戻したらこうなる」を示せる。
 */
const BUILTIN_CATEGORY: Record<string, string> = {
  branch_pushed: 'started',
  pr_opened: 'review',
  pr_ready: 'review',
  pr_merged: 'completed',
};

export function builtinLabelForTrigger(trigger: string): BuiltinLabel {
  const category = BUILTIN_CATEGORY[trigger];
  if (!category) {
    return { text: 'none', i18nKey: 'integration.gitRules.none' };
  }
  return { text: category, i18nKey: `settings.categoryLabels.${category}` };
}

/** 画面での並び順: 開発が進む順(push → PR → レビュー・CI → マージ)。API の返す順とは別。 */
export const TRIGGER_DISPLAY_ORDER = [
  'branch_pushed',
  'pr_draft',
  'pr_opened',
  'pr_ready',
  'pr_ci_failed',
  'pr_changes_requested',
  'pr_review_approved',
  'pr_merged',
  'pr_closed',
] as const;

export function triggerRank(trigger: string): number {
  const i = (TRIGGER_DISPLAY_ORDER as readonly string[]).indexOf(trigger);
  return i === -1 ? TRIGGER_DISPLAY_ORDER.length : i;
}

/** きっかけごとのアイコン(文字を読まなくても、何の場面か分かるように) */
export const TRIGGER_ICONS: Record<string, string> = {
  branch_pushed: '🌿',
  pr_draft: '📝',
  pr_opened: '🔀',
  pr_ready: '👀',
  pr_ci_failed: '❌',
  pr_changes_requested: '✋',
  pr_review_approved: '✅',
  pr_merged: '🎉',
  pr_closed: '🚫',
};

export function triggerIcon(trigger: string): string {
  return TRIGGER_ICONS[trigger] ?? '•';
}

/**
 * 「この設定だと、チケットはどうなるか」(行の下に出す一文の元)。
 * `status` は動かす先の表示名。何もしない場合は null。
 * `categoryName` は、カテゴリ名から表示名を作る関数(翻訳は画面側で行う)。
 */
export type Preview =
  | { kind: 'none' }
  /** 動かす先の実際のステータス名 */
  | { kind: 'move'; status: string }
  /** そのカテゴリにあたるステータスが、このプロジェクト/チームの流れに無い(動かさない) */
  | { kind: 'missing'; category: string };

export function previewStatusName(
  value: RuleValue,
  trigger: string,
  statuses: GitStatusRuleStatus[],
  builtinCategory: (trigger: string) => string | null,
): Preview {
  if (value === 'none') return { kind: 'none' };
  const byCategory = (c: string): Preview => {
    const st = statusForCategory(statuses, c);
    return st ? { kind: 'move', status: st.name } : { kind: 'missing', category: c };
  };
  if (value === 'default') {
    const c = builtinCategory(trigger);
    return c ? byCategory(c) : { kind: 'none' };
  }
  if (value.startsWith('category:')) return byCategory(value.slice('category:'.length));
  if (value.startsWith('slug:')) {
    const slug = value.slice('slug:'.length);
    return { kind: 'move', status: statuses.find((st) => st.slug === slug)?.name ?? slug };
  }
  return { kind: 'none' };
}

/** 標準の動作の「カテゴリ名」。何もしないものは null。 */
export function builtinCategoryOf(trigger: string): string | null {
  return BUILTIN_CATEGORY[trigger] ?? null;
}

/**
 * カテゴリから、そのプロジェクト/チームの「実際のステータス名」を探す(バックエンドの解決と同じ規則)。
 * - 通常のカテゴリ: そのカテゴリの先頭のステータス(statuses は並び順どおり)
 * - review: 着手中カテゴリの in_review / review / in-review → review カテゴリ → 名前が in_review 等のもの
 * 見つからなければ null(その場合、規則は動かさない)。
 */
export function statusForCategory(
  statuses: GitStatusRuleStatus[],
  category: string,
): GitStatusRuleStatus | null {
  if (category !== 'review') {
    return statuses.find((s) => s.category === category) ?? null;
  }
  const reviewSlugs = ['in_review', 'review', 'in-review'];
  return (
    statuses.find((s) => s.category === 'started' && reviewSlugs.includes(s.slug)) ??
    statuses.find((s) => s.category === 'review') ??
    statuses.find((s) => reviewSlugs.includes(s.slug)) ??
    null
  );
}
