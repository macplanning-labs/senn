/**
 * gitRules.test.ts — Git ステータスルール ユーティリティ テスト
 */

import { describe, it, expect } from 'vitest';
import {
  TRIGGER_DISPLAY_ORDER,
  builtinCategoryOf,
  previewStatusName,
  statusForCategory,
  triggerIcon,
  triggerRank,
  builtinLabelForTrigger,
  ruleToForm,
  formToRule,
  collectChangedRules,
  generateOptionGroups,
  type RuleForm,
} from './gitRules';
import type { GitStatusRule } from '@/shared/api/types';

describe('gitRules', () => {
  describe('ruleToForm', () => {
    it('デフォルトルールを "default" に変換', () => {
      const rule: GitStatusRule = {
        trigger: 'branch_pushed',
        action: 'set',
        targetSlug: null,
        targetCategory: 'started',
        isDefault: true,
      };
      const form = ruleToForm(rule);
      expect(form.trigger).toBe('branch_pushed');
      expect(form.value).toBe('default');
    });

    it('"none" ルールを変換', () => {
      const rule: GitStatusRule = {
        trigger: 'pr_draft',
        action: 'none',
        targetSlug: null,
        targetCategory: null,
        isDefault: false,
      };
      const form = ruleToForm(rule);
      expect(form.value).toBe('none');
    });

    it('カテゴリ指定ルールを変換', () => {
      const rule: GitStatusRule = {
        trigger: 'pr_merged',
        action: 'set',
        targetSlug: null,
        targetCategory: 'completed',
        isDefault: false,
      };
      const form = ruleToForm(rule);
      expect(form.value).toBe('category:completed');
    });

    it('スラッグ指定ルールを変換', () => {
      const rule: GitStatusRule = {
        trigger: 'pr_opened',
        action: 'set',
        targetSlug: 'in-review',
        targetCategory: null,
        isDefault: false,
      };
      const form = ruleToForm(rule);
      expect(form.value).toBe('slug:in-review');
    });
  });

  describe('formToRule', () => {
    it('デフォルト形式は null を返す', () => {
      const form: RuleForm = { trigger: 'branch_pushed', value: 'default' };
      expect(formToRule(form)).toBeNull();
    });

    it('"none" 形式を変換', () => {
      const form: RuleForm = { trigger: 'pr_draft', value: 'none' };
      const rule = formToRule(form);
      expect(rule).toEqual({
        trigger: 'pr_draft',
        action: 'none',
      });
    });

    it('カテゴリ形式を変換', () => {
      const form: RuleForm = { trigger: 'pr_merged', value: 'category:completed' };
      const rule = formToRule(form);
      expect(rule).toEqual({
        trigger: 'pr_merged',
        action: 'set',
        targetCategory: 'completed',
      });
    });

    it('スラッグ形式を変換', () => {
      const form: RuleForm = { trigger: 'pr_opened', value: 'slug:in-review' };
      const rule = formToRule(form);
      expect(rule).toEqual({
        trigger: 'pr_opened',
        action: 'set',
        targetSlug: 'in-review',
      });
    });
  });

  describe('collectChangedRules', () => {
    it('デフォルト以外のルール(変更)だけを返す', () => {
      const forms: RuleForm[] = [
        { trigger: 'branch_pushed', value: 'default' },
        { trigger: 'pr_merged', value: 'category:completed' },
        { trigger: 'pr_draft', value: 'none' },
      ];
      const changed = collectChangedRules(forms);
      expect(changed).toHaveLength(2);
      expect(changed[0].trigger).toBe('pr_merged');
      expect(changed[1].trigger).toBe('pr_draft');
    });

    it('変更がない場合は空配列', () => {
      const forms: RuleForm[] = [{ trigger: 'branch_pushed', value: 'default' }];
      const changed = collectChangedRules(forms);
      expect(changed).toHaveLength(0);
    });
  });

  describe('generateOptionGroups', () => {
    it('基本グループ・カテゴリグループ・ステータスグループを生成', () => {
      const statuses = [
        { slug: 'backlog', name: 'Backlog', category: 'backlog' },
        { slug: 'in-progress', name: 'In Progress', category: 'started' },
      ];
      const groups = generateOptionGroups(statuses);

      // 3 グループ
      expect(groups).toHaveLength(3);

      // 基本グループ(ラベル無し)
      expect(groups[0].label).toBeUndefined();
      expect(groups[0].options).toHaveLength(2);

      // カテゴリグループ
      expect(groups[1].label).toBe('integration.gitRules.categoryLabel');
      expect(groups[1].options).toHaveLength(6); // 6 カテゴリ

      // ステータスグループ
      expect(groups[2].label).toBe('integration.gitRules.statusLabel');
      expect(groups[2].options).toHaveLength(2);
    });
  });

});

describe('builtinLabelForTrigger', () => {
  it('標準の動作(Phase 1 までと同じ)を返す', () => {
    expect(builtinLabelForTrigger('branch_pushed').text).toBe('started');
    expect(builtinLabelForTrigger('pr_opened').text).toBe('review');
    expect(builtinLabelForTrigger('pr_ready').text).toBe('review');
    expect(builtinLabelForTrigger('pr_merged').text).toBe('completed');
  });

  it('標準で何もしないトリガーは「何もしない」', () => {
    for (const t of ['pr_draft', 'pr_review_approved', 'pr_changes_requested', 'pr_ci_failed', 'pr_closed']) {
      expect(builtinLabelForTrigger(t).i18nKey).toBe('integration.gitRules.none');
    }
  });
});

describe('表示順・アイコン', () => {
  it('開発が進む順に並ぶ(push → PR → CI・レビュー → マージ)', () => {
    const sorted = ['pr_merged', 'branch_pushed', 'pr_ci_failed', 'pr_opened'].sort(
      (a, b) => triggerRank(a) - triggerRank(b),
    );
    expect(sorted).toEqual(['branch_pushed', 'pr_opened', 'pr_ci_failed', 'pr_merged']);
  });

  it('9 つのトリガーすべてに、アイコンがある', () => {
    expect(TRIGGER_DISPLAY_ORDER.length).toBe(9);
    for (const t of TRIGGER_DISPLAY_ORDER) {
      expect(triggerIcon(t)).not.toBe('•');
    }
  });
});

describe('statusForCategory / previewStatusName(カテゴリは、実際のステータス名で示す)', () => {
  const statuses = [
    { slug: 'backlog', name: 'Backlog', category: 'backlog' },
    { slug: 'open', name: 'Todo', category: 'unstarted' },
    { slug: 'in_progress', name: 'In Progress', category: 'started' },
    { slug: 'resolved', name: 'Resolved', category: 'completed' },
    { slug: 'closed', name: 'Closed', category: 'completed' },
  ];

  it('完了カテゴリは、先頭のステータス(Resolved)になる', () => {
    expect(statusForCategory(statuses, 'completed')?.name).toBe('Resolved');
    expect(statusForCategory(statuses, 'started')?.name).toBe('In Progress');
  });

  it('review は in_review 等を探し、無ければ null', () => {
    expect(statusForCategory(statuses, 'review')).toBeNull();
    const withReview = [...statuses, { slug: 'in_review', name: 'In Review', category: 'started' }];
    expect(statusForCategory(withReview, 'review')?.name).toBe('In Review');
  });

  it('何もしない → none', () => {
    expect(previewStatusName('none', 'pr_merged', statuses, builtinCategoryOf)).toEqual({ kind: 'none' });
  });

  it('標準 → 標準のカテゴリの実際のステータス名(マージ → Resolved)。標準で何もしないものは none', () => {
    expect(previewStatusName('default', 'pr_merged', statuses, builtinCategoryOf)).toEqual({ kind: 'move', status: 'Resolved' });
    expect(previewStatusName('default', 'pr_draft', statuses, builtinCategoryOf)).toEqual({ kind: 'none' });
  });

  it('標準がレビュー相当で、レビューのステータスが無ければ missing(動かさない)', () => {
    expect(previewStatusName('default', 'pr_opened', statuses, builtinCategoryOf)).toEqual({ kind: 'missing', category: 'review' });
  });

  it('カテゴリ・ステータスを選んだら、その実際の名前', () => {
    expect(previewStatusName('category:completed', 'pr_opened', statuses, builtinCategoryOf)).toEqual({ kind: 'move', status: 'Resolved' });
    expect(previewStatusName('slug:closed', 'pr_opened', statuses, builtinCategoryOf)).toEqual({ kind: 'move', status: 'Closed' });
  });

  it('流れに無い slug は slug 名のまま(落ちない)', () => {
    expect(previewStatusName('slug:gone', 'pr_opened', statuses, builtinCategoryOf)).toEqual({ kind: 'move', status: 'gone' });
  });
});
