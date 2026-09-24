/**
 * ProjectOverview.tsx - プロジェクト概要ページ
 *
 * プロジェクト詳細情報（説明、ステータス、優先度、期日、チーム、チケット数）
 * マイルストーン一覧を表示
 */

import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useProject } from '@/shared/hooks/useProject';
import { apiClient } from '@/shared/api/client';
import { ProjectUpdatesSection } from './ProjectUpdatesSection';
import { teamBadgeLabelKey } from '../utils/teamBadge';
import './ProjectOverview.css';

interface Milestone {
  id: number;
  name: string;
  dueDate?: string | null;
}

export function ProjectOverview() {
  const { t } = useTranslation();
  const { currentProject, isLoading: projectLoading } = useProject();

  // マイルストーン取得
  const { data: milestonesData, isLoading: milestonesLoading } = useQuery<{
    results: Milestone[];
  }>({
    queryKey: ['milestones', currentProject?.id],
    queryFn: async () => {
      const res = await apiClient.get<{ results: Milestone[] }>('/milestones/', {
        params: { project: currentProject?.id },
      });
      return res.data;
    },
    enabled: !!currentProject?.id,
    staleTime: 1000 * 60 * 5, // 5分キャッシュ
  });

  const milestones = milestonesData?.results ?? [];

  if (projectLoading) {
    return <div className="project-overview__loading">{t('common.loading')}</div>;
  }

  if (!currentProject) {
    return <div className="project-overview__error">{t('projectTabs.notFound')}</div>;
  }

  // 日付をフォーマット
  const formatDate = (dateStr?: string | null) => {
    if (!dateStr) return null;
    try {
      return new Date(dateStr).toLocaleDateString();
    } catch {
      return dateStr;
    }
  };

  return (
    <div className="project-overview">
      {/* 進捗報告 */}
      <ProjectUpdatesSection projectId={currentProject.id} isMember={currentProject.isMember} />

      {/* 説明セクション */}
      <section className="project-overview__section">
        <h2 className="project-overview__section-title">
          {t('projectTabs.overviewDescription')}
        </h2>
        <div className="project-overview__description">
          {currentProject.description && currentProject.description.trim()
            ? currentProject.description
            : t('projectTabs.overviewNoDescription')}
        </div>
      </section>

      {/* 詳細セクション */}
      <section className="project-overview__section">
        <h2 className="project-overview__section-title">
          {t('projectTabs.overviewAttributes')}
        </h2>
        <div className="project-overview__attributes">
          {/* ステータス */}
          <div className="project-overview__attribute">
            <span className="project-overview__attribute-label">
              {t('projectTabs.overviewStatus')}
            </span>
            <span className="project-overview__attribute-value">
              {currentProject.status || t('projectTabs.overviewNotSet')}
            </span>
          </div>

          {/* 優先度 */}
          <div className="project-overview__attribute">
            <span className="project-overview__attribute-label">
              {t('projectTabs.overviewPriority')}
            </span>
            <span className="project-overview__attribute-value">
              {currentProject.priority || t('projectTabs.overviewNotSet')}
            </span>
          </div>

          {/* ターゲット期日 */}
          <div className="project-overview__attribute">
            <span className="project-overview__attribute-label">
              {t('projectTabs.overviewTargetDate')}
            </span>
            <span className="project-overview__attribute-value">
              {currentProject.targetEndDate
                ? formatDate(currentProject.targetEndDate)
                : t('projectTabs.overviewNotSet')}
            </span>
          </div>

          {/* チーム */}
          <div className="project-overview__attribute">
            <span className="project-overview__attribute-label">
              {t('projectTabs.overviewTeams')}
            </span>
            <div className="project-overview__teams">
              {currentProject.teams && currentProject.teams.length > 0
                ? currentProject.teams.map((team) => {
                    const badgeKey = teamBadgeLabelKey(team);
                    return (
                      <span
                        key={team.id}
                        className="project-overview__team-badge"
                        style={{
                          backgroundColor: team.color || 'var(--color-bg-secondary)',
                          color: 'var(--color-text-primary)',
                        }}
                      >
                        {team.name}
                        {badgeKey && (
                          <span
                            className="project-overview__team-archived"
                            data-testid={`project-overview-team-archived-${team.id}`}
                          >
                            {t(badgeKey)}
                          </span>
                        )}
                      </span>
                    );
                  })
                : t('projectTabs.overviewNotSet')}
            </div>
          </div>

          {/* チケット数 */}
          <div className="project-overview__attribute">
            <span className="project-overview__attribute-label">
              {t('projectTabs.overviewTicketCount')}
            </span>
            <span className="project-overview__attribute-value">
              {currentProject.ticketCount ?? t('projectTabs.overviewNotSet')}
            </span>
          </div>
        </div>
      </section>

      {/* マイルストーンセクション */}
      <section className="project-overview__section">
        <h2 className="project-overview__section-title">
          {t('projectTabs.overviewMilestones')}
        </h2>
        {milestonesLoading ? (
          <div className="project-overview__loading">{t('common.loading')}</div>
        ) : milestones.length > 0 ? (
          <div className="project-overview__milestones">
            {milestones.map((milestone) => (
              <div key={milestone.id} className="project-overview__milestone">
                <div className="project-overview__milestone-name">{milestone.name}</div>
                {milestone.dueDate && (
                  <div className="project-overview__milestone-date">
                    {formatDate(milestone.dueDate)}
                  </div>
                )}
              </div>
            ))}
          </div>
        ) : (
          <div className="project-overview__empty">
            {t('projectTabs.overviewNoMilestones')}
          </div>
        )}
      </section>
    </div>
  );
}
