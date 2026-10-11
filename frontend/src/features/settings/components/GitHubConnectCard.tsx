/**
 * GitHubConnectCard.tsx — GitHub App 連携のかんたん接続
 *
 * Project または Team 設定で GitHub リポジトリを選択・リンク。
 * App 作成→インストール→リポジトリ選択の流れ。
 */

import { useEffect, useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { GitHubAppStatus, GitHubRepo } from '@/shared/api/types';
import { useToast } from '@/shared/stores/toastStore';
import { useTranslation } from 'react-i18next';
import { submitManifestForm, filterRepositories, shouldWarnNotMember } from '../utils/github';
import './GitHubConnectCard.css';

interface GitHubConnectCardProps {
  projectId?: number;
  teamId?: number;
}

export function GitHubConnectCard({ projectId, teamId }: GitHubConnectCardProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const toast = useToast();

  const [orgName, setOrgName] = useState('');
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedRepos, setSelectedRepos] = useState<Set<string>>(new Set());

  const scopeKey = projectId != null ? `project:${projectId}` : `team:${teamId}`;

  // GitHub App の状態を取得
  const {
    data: appStatus,
    isLoading: isLoadingStatus,
    isError: isErrorStatus,
  } = useQuery<GitHubAppStatus>({
    queryKey: ['github-app', 'status', scopeKey],
    queryFn: async () => {
      // 範囲を渡すと、サーバーが「閲覧者がそのチームのメンバーか」も返す
      const params = new URLSearchParams();
      if (projectId != null) params.append('project', String(projectId));
      if (teamId != null) params.append('team', String(teamId));
      const res = await apiClient.get(`/github-app/status/?${params.toString()}`);
      return res.data;
    },
  });

  // リポジトリ一覧を取得
  const {
    data: repositories = [],
    isLoading: isLoadingRepos,
  } = useQuery<GitHubRepo[]>({
    queryKey: ['github-app', 'repositories', scopeKey],
    queryFn: async () => {
      const params = new URLSearchParams();
      if (projectId != null) params.append('project', String(projectId));
      if (teamId != null) params.append('team', String(teamId));
      const res = await apiClient.get(`/github-app/repositories/?${params.toString()}`);
      return res.data;
    },
    enabled:
      appStatus != null &&
      appStatus.configured &&
      (appStatus.installations?.length ?? 0) > 0 &&
      (projectId != null || teamId != null),
  });

  // App Manifest を生成
  const manifestMutation = useMutation({
    mutationFn: async (data: { org?: string; appName?: string }) => {
      const returnPath = window.location.pathname + window.location.search;
      const res = await apiClient.post('/github-app/manifest/', { ...data, returnPath });
      return res.data;
    },
    onSuccess: (data) => {
      submitManifestForm(data.postUrl, data.manifest);
    },
    onError: (err) => {
      // サーバーが理由を返したとき(例: この環境のアドレスでは GitHub App を作れない)は、その理由を出す
      const detail = (err as { response?: { data?: { detail?: unknown } } })?.response?.data?.detail;
      toast.error(typeof detail === 'string' && detail ? detail : t('integration.github.failed'));
    },
  });

  // インストール用の URL を取得
  const installMutation = useMutation({
    mutationFn: async () => {
      const returnPath = window.location.pathname + window.location.search;
      const res = await apiClient.post('/github-app/install/', { returnPath });
      return res.data;
    },
    onSuccess: (data) => {
      window.location.assign(data.url);
    },
    onError: () => {
      toast.error(t('settings.updateFailed'));
    },
  });

  // リポジトリを連携(選んだリポジトリを、所属するインストールごとにまとめて送る)
  const integrateMutation = useMutation({
    mutationFn: async () => {
      const byInstallation = new Map<number, string[]>();
      for (const repo of repositories) {
        if (!selectedRepos.has(repo.fullName)) continue;
        const list = byInstallation.get(repo.installationId) ?? [];
        list.push(repo.fullName);
        byInstallation.set(repo.installationId, list);
      }
      if (byInstallation.size === 0) {
        throw new Error('No repositories selected');
      }
      for (const [installationId, names] of byInstallation) {
        await apiClient.post('/github-app/integrations/', {
          ...(projectId != null ? { project: projectId } : {}),
          ...(teamId != null ? { team: teamId } : {}),
          installationId,
          repositories: names,
        });
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['integrations', scopeKey] });
      void queryClient.invalidateQueries({ queryKey: ['github-app', 'repositories', scopeKey] });
      setSelectedRepos(new Set());
      toast.success(t('integration.github.success'));
    },
    onError: () => {
      toast.error(t('integration.github.failed'));
    },
  });

  // URL のクエリパラメータから状態を確認(確認したら githubApp だけ消す)
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const result = params.get('githubApp');
    if (result !== 'created' && result !== 'installed' && result !== 'error') return;

    if (result === 'error') {
      toast.error(t('integration.github.appError'));
    } else {
      toast.success(
        t(result === 'created' ? 'integration.github.appCreated' : 'integration.github.appInstalled'),
      );
    }
    params.delete('githubApp');
    const rest = params.toString();
    history.replaceState(null, '', window.location.pathname + (rest ? `?${rest}` : ''));
    void queryClient.invalidateQueries({ queryKey: ['github-app'] });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // フィルター済みリポジトリ
  const filteredRepos = filterRepositories(repositories, searchQuery);

  if (isLoadingStatus) {
    return <div className="github-card github-card--loading">{t('integration.github.diagnostics.loading')}</div>;
  }

  if (isErrorStatus || !appStatus) {
    return null;
  }

  return (
    <div className="github-card">
      <div className="github-card__header">
        <h3 className="github-card__title">🐙 {t('integration.github.title')}</h3>
        <p className="github-card__description">{t('integration.github.description')}</p>
      </div>

      {/* メンバーではない人(システム管理者など)が連携を作ろうとしているとき */}
      {shouldWarnNotMember(appStatus) && (
        <div className="github-card__notice" role="status">
          <p>{t('integration.github.notMemberNotice')}</p>
        </div>
      )}

      {/* App 未作成 & 作成可能 */}
      {!appStatus.configured && appStatus.canCreateApp && (
        <div className="github-card__section">
          <p className="github-card__help">{t('integration.github.createHelp')}</p>
          <div className="github-card__input-group">
            <input
              type="text"
              placeholder={t('integration.github.orgPlaceholder')}
              value={orgName}
              onChange={(e) => setOrgName(e.target.value)}
              className="github-card__input"
            />
          </div>
          {appStatus.createBlockedReason && (
            <div className="github-card__notice" role="status">
              <p>{appStatus.createBlockedReason}</p>
            </div>
          )}
          <button
            onClick={() => manifestMutation.mutate({ org: orgName || undefined })}
            disabled={manifestMutation.isPending || !!appStatus.createBlockedReason}
            className="github-card__button github-card__button--primary"
          >
            {t('integration.github.startButton')}
          </button>
        </div>
      )}

      {/* App 未作成 & 作成不可 */}
      {!appStatus.configured && !appStatus.canCreateApp && (
        <div className="github-card__notice">
          <p>{t('integration.github.needsAdminSetup')}</p>
        </div>
      )}

      {/* App 作成済み & インストール無し */}
      {appStatus.configured &&
        appStatus.app &&
        (!appStatus.installations || appStatus.installations.length === 0) && (
          <div className="github-card__section">
            <p className="github-card__help">{t('integration.github.selectReposHelp')}</p>
            <button
              onClick={() => installMutation.mutate()}
              disabled={installMutation.isPending}
              className="github-card__button github-card__button--primary"
            >
              {t('integration.github.selectRepos')}
            </button>
          </div>
        )}

      {/* インストール有り & リポジトリ表示 */}
      {appStatus.configured &&
        appStatus.app &&
        appStatus.installations &&
        appStatus.installations.length > 0 && (
          <div className="github-card__section">
            <p className="github-card__help">{t('integration.github.selectReposLabel')}</p>

            <div className="github-card__search">
              <input
                type="text"
                placeholder={t('integration.github.searchPlaceholder')}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="github-card__search-input"
              />
            </div>

            {isLoadingRepos ? (
              <div className="github-card__loading">{t('integration.github.diagnostics.loading')}</div>
            ) : (
              <div className="github-card__repo-list">
                {filteredRepos.length === 0 ? (
                  <p className="github-card__empty">{t('integration.github.noRepos')}</p>
                ) : (
                  filteredRepos.map((repo) => (
                    <label key={repo.fullName} className="github-card__repo-item">
                      <input
                        type="checkbox"
                        checked={selectedRepos.has(repo.fullName)}
                        onChange={(e) => {
                          const next = new Set(selectedRepos);
                          if (e.target.checked) {
                            next.add(repo.fullName);
                          } else {
                            next.delete(repo.fullName);
                          }
                          setSelectedRepos(next);
                        }}
                        disabled={repo.linked}
                        className="github-card__checkbox"
                      />
                      <span className="github-card__repo-name">{repo.fullName}</span>
                      {repo.private && <span className="github-card__repo-badge">🔒 {t('integration.github.privateBadge')}</span>}
                      {repo.linked && <span className="github-card__repo-badge--linked">✓ {t('integration.github.linkedBadge')}</span>}
                    </label>
                  ))
                )}
              </div>
            )}

            <div className="github-card__actions">
              <button
                onClick={() => integrateMutation.mutate()}
                disabled={selectedRepos.size === 0 || integrateMutation.isPending}
                className="github-card__button github-card__button--primary"
              >
                {integrateMutation.isPending ? t('integration.github.linking') : t('integration.github.confirm')}
              </button>
              <button
                onClick={() => installMutation.mutate()}
                className="github-card__button github-card__button--secondary"
              >
                {t('integration.github.addMore')}
              </button>
            </div>
          </div>
        )}
    </div>
  );
}
