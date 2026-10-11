/**
 * GitStatusRules.tsx — GitHub イベント → ステータス自動更新ルール設定UI
 *
 * Project または Team 設定の Git連携タブで、自動ステータス更新の規則を設定。
 */

import { useEffect, useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { ForbiddenNotice, isForbiddenError } from './ForbiddenNotice';
import type { GitStatusRulesResponse } from '@/shared/api/types';
import { useToast } from '@/shared/stores/toastStore';
import { useTranslation } from 'react-i18next';
import {
  ruleToForm,
  collectChangedRules,
  generateOptionGroups,
  builtinLabelForTrigger,
  builtinCategoryOf,
  previewStatusName,
  statusForCategory,
  triggerIcon,
  triggerRank,
  type RuleForm,
} from '../utils/gitRules';
import './GitHubConnectCard.css';
import './GitStatusRules.css';

interface GitStatusRulesProps {
  projectId?: number;
  teamId?: number;
}

export function GitStatusRules({ projectId, teamId }: GitStatusRulesProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const toast = useToast();

  const [rules, setRules] = useState<RuleForm[]>([]);
  const [hasChanges, setHasChanges] = useState(false);

  const scopeKey = projectId != null ? `project:${projectId}` : `team:${teamId}`;

  // ルールとステータスを取得
  const {
    data: rulesData,
    isLoading,
    error: loadError,
  } = useQuery<GitStatusRulesResponse>({
    queryKey: ['git-status-rules', scopeKey],
    queryFn: async () => {
      const params = new URLSearchParams();
      if (projectId != null) params.append('project', String(projectId));
      if (teamId != null) params.append('team', String(teamId));
      const res = await apiClient.get(`/git-status-rules/?${params.toString()}`);
      return res.data;
    },
    enabled: projectId != null || teamId != null,
  });

  // ルール取得後、フォームを初期化
  useEffect(() => {
    if (rulesData) {
      const formRules = rulesData.rules.map(ruleToForm);
      setRules(formRules);
      setHasChanges(false);
    }
  }, [rulesData]);

  const saveMutation = useMutation({
    mutationFn: async () => {
      const changedRules = collectChangedRules(rules);
      const res = await apiClient.put('/git-status-rules/', {
        project: projectId,
        team: teamId,
        rules: changedRules,
      });
      return res.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['git-status-rules', scopeKey] });
      setHasChanges(false);
      toast.success(t('integration.gitRules.saved'));
    },
    onError: (err: any) => {
      if (err.response?.data?.detail) {
        toast.error(err.response.data.detail);
      } else {
        toast.error(t('integration.gitRules.failed'));
      }
    },
  });

  const handleRuleChange = (index: number, value: string) => {
    const newRules = [...rules];
    if (newRules[index]) {
      newRules[index].value = value as any;
      setRules(newRules);
      setHasChanges(true);
    }
  };

  const handleResetAll = async () => {
    // eslint-disable-next-line no-alert
    if (window.confirm(t('integration.gitRules.resetAllConfirm'))) {
      const newRules = rules.map((r) => ({ ...r, value: 'default' as const }));
      setRules(newRules);
      setHasChanges(true);
      // 実際に保存する
      try {
        const changedRules = collectChangedRules(newRules);
        await apiClient.put('/git-status-rules/', {
          project: projectId,
          team: teamId,
          rules: changedRules,
        });
        void queryClient.invalidateQueries({ queryKey: ['git-status-rules', scopeKey] });
        setHasChanges(false);
        toast.success(t('integration.gitRules.saved'));
      } catch (err: any) {
        if (err.response?.data?.detail) {
          toast.error(err.response.data.detail);
        } else {
          toast.error(t('integration.gitRules.failed'));
        }
      }
    }
  };

  if (isForbiddenError(loadError)) {
    return <ForbiddenNotice />;
  }

  if (loadError && !rulesData) {
    return <div className="git-rules-loading">{t('integration.gitRules.loadFailed')}</div>;
  }

  if (isLoading || !rulesData) {
    return <div className="git-rules-loading">{t('common.loading')}</div>;
  }

  const optionGroups = generateOptionGroups(rulesData.statuses);

  return (
    <div className="github-card">
      <div className="github-card__header">
        <h3 className="github-card__title">{t('integration.gitRules.heading')}</h3>
        <p className="github-card__description">{t('integration.gitRules.description')}</p>
      </div>

      <div className="git-rules-table">
        <div className="git-rules-table-header">
          <div>{t('integration.gitRules.trigger')}</div>
          <div aria-hidden="true" />
          <div>{t('integration.gitRules.action')}</div>
        </div>

        {rules
          .map((rule, idx) => ({ rule, idx }))
          .sort((x, y) => triggerRank(x.rule.trigger) - triggerRank(y.rule.trigger))
          .map(({ rule, idx }) => {
            const builtinInfo = builtinLabelForTrigger(rule.trigger);
            // カテゴリの表示名 + そのプロジェクトの実際のステータス名(例: 完了(Resolved))
            const withActual = (category: string) => {
              const st = statusForCategory(rulesData.statuses, category);
              const label = t(`settings.categoryLabels.${category}`);
              return st ? `${label}(${st.name})` : `${label}(${t('integration.gitRules.noStatus')})`;
            };
            // 「標準」の選択肢は、実際にどのステータスになるかを名前で示す(例: Resolved(標準))
            const builtinCat = builtinCategoryOf(rule.trigger);
            const builtinStatus = builtinCat ? statusForCategory(rulesData.statuses, builtinCat) : null;
            const builtinText = builtinCat
              ? (builtinStatus?.name ?? t('integration.gitRules.noStatus'))
              : t(builtinInfo.i18nKey ?? 'integration.gitRules.none');
            const optionLabel = (opt: { value: string; label: string; i18nKey?: string }) => {
              if (opt.value === 'default') return `${builtinText}${t('integration.gitRules.defaultSuffix')}`;
              if (opt.value.startsWith('category:')) return withActual(opt.value.slice('category:'.length));
              return opt.i18nKey ? t(opt.i18nKey) : opt.label;
            };
            const preview = previewStatusName(
              rule.value,
              rule.trigger,
              rulesData.statuses,
              builtinCategoryOf,
            );

            return (
              <div key={rule.trigger} className="git-rules-table-row">
                <div className="git-rules-trigger-col">
                  <span className="git-rules-dot" aria-hidden="true">
                    {triggerIcon(rule.trigger)}
                  </span>
                  <span className="git-rules-trigger-name">
                    {t(`integration.gitRules.triggers.${rule.trigger}`)}
                  </span>
                </div>
                <div className="git-rules-arrow" aria-hidden="true">
                  ➔
                </div>
                <div className="git-rules-action-col">
                  <select
                    value={rule.value}
                    onChange={(e) => handleRuleChange(idx, e.target.value)}
                    className="git-rules-select"
                    aria-label={t(`integration.gitRules.triggers.${rule.trigger}`)}
                  >
                    {optionGroups.map((group, groupIdx) =>
                      group.label ? (
                        <optgroup key={groupIdx} label={t(group.label)}>
                          {group.options.map((opt) => (
                            <option key={opt.value} value={opt.value}>
                              {optionLabel(opt)}
                            </option>
                          ))}
                        </optgroup>
                      ) : (
                        group.options.map((opt) => (
                          <option key={opt.value} value={opt.value}>
                            {optionLabel(opt)}
                          </option>
                        ))
                      ),
                    )}
                  </select>
                  <div className="git-rules-preview">
                    {preview.kind === 'move' && t('integration.gitRules.previewMove', { status: preview.status })}
                    {preview.kind === 'none' && t('integration.gitRules.previewNone')}
                    {preview.kind === 'missing' &&
                      t('integration.gitRules.previewMissing', {
                        category: t(`settings.categoryLabels.${preview.category}`),
                      })}
                  </div>
                </div>
              </div>
            );
          })}
      </div>

      <div className="git-rules-actions">
        <button
          onClick={() => saveMutation.mutate()}
          disabled={!hasChanges || saveMutation.isPending}
          className="github-card__button github-card__button--primary"
        >
          {saveMutation.isPending ? t('common.saving') : t('integration.gitRules.save')}
        </button>
        <button
          onClick={handleResetAll}
          disabled={saveMutation.isPending}
          className="github-card__button github-card__button--secondary"
        >
          {t('integration.gitRules.resetAll')}
        </button>
      </div>
    </div>
  );
}
