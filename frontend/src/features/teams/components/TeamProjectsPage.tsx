/**
 * TeamProjectsPage.tsx — チーム配下の Projects 一覧
 *
 * STEP3 でサンプルデータボタンとバナー、削除機能を追加。
 */

import { useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useQueryClient } from '@tanstack/react-query';
import { useProjects } from '@/shared/sync/repos/projectRepo';
import { useTeam } from '@/shared/hooks/useTeam';
import { ProjectsTable } from '@/features/projects/components/ProjectsTable';
import { ProjectCreateModal } from '@/features/projects/components/ProjectCreateModal';
import { DemoDataButton } from '@/features/onboarding/components/DemoDataButton';
import { DemoReadyMessage } from '@/features/onboarding/components/DemoReadyMessage';
import { isDemoProject, deleteDemoProject } from '@/features/onboarding/demoData';
import { useToast } from '@/shared/stores/toastStore';
import './TeamProjectsPage.css';
import { TeamTabPageHeader } from '@/features/teams/components/TeamTabPageHeader';
import { IconFolder } from '@/shared/components/layout/Sidebar';

export function TeamProjectsPage() {
  const { t } = useTranslation();
  const { teamSlug } = useParams<{ teamSlug: string }>();
  const { currentTeam } = useTeam();
  const { projects: projectList, isLoading } = useProjects();
  const queryClient = useQueryClient();
  const { success: showSuccess, error: showError } = useToast();

  const [projectModalOpen, setProjectModalOpen] = useState(false);
  const [createdPrefix, setCreatedPrefix] = useState<string | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<number | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  const teamId = currentTeam?.id;
  const projects = projectList.filter((p) => {
    if (!teamId) return false;
    return (p.teams ?? []).some((t) => t.id === teamId);
  });

  // デモプロジェクトを検索
  const demoProject = projects.find((p) => isDemoProject(p));

  // 削除処理
  const handleDeleteDemo = async (projectId: number) => {
    setIsDeleting(true);
    try {
      await deleteDemoProject(projectId);

      // キャッシュを無効化（['projects'] は端末内 DB が自動更新）
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });

      showSuccess(t('onboarding.deleted'));
      setDeleteConfirm(null);
      // 削除済みプロジェクトを指す完了メッセージが残らないようにする
      setCreatedPrefix(null);
    } catch (err) {
      showError(t('onboarding.deleteFailed'));
      console.error('Demo data deletion failed:', err);
    } finally {
      setIsDeleting(false);
    }
  };

  const existingPrefixes = projectList.map((p) => p.prefix);

  return (
    <div className="team-projects" data-testid="team-projects-page">
      <TeamTabPageHeader
        icon={IconFolder}
        title={t('nav.projects')}
        subtitle={<p className="team-projects__subtitle">{currentTeam?.name ?? teamSlug}</p>}
      />

      {/* デモプロジェクトバナー */}
      {demoProject && !createdPrefix && (
        <div className="team-projects__demo-banner">
          <p className="team-projects__demo-banner-text">{t('onboarding.demoBanner')}</p>
          <div className="team-projects__demo-banner-actions">
            <Link
              to={`/project/${demoProject.prefix}/gantt`}
              className="team-projects__demo-banner-link"
            >
              {t('onboarding.openGantt')}
            </Link>
            <Link
              to={`/project/${demoProject.prefix}/dependencies`}
              className="team-projects__demo-banner-link"
            >
              {t('onboarding.openDependencies')}
            </Link>
            <button
              type="button"
              data-testid="team-projects-delete-demo"
              className="team-projects__demo-banner-delete"
              onClick={() => setDeleteConfirm(demoProject.id)}
            >
              {t('onboarding.deleteDemo')}
            </button>
          </div>
        </div>
      )}

      {/* サンプル生成完了メッセージ */}
      {createdPrefix && (
        <div className="team-projects__demo-ready">
          <DemoReadyMessage prefix={createdPrefix} />
        </div>
      )}

      {isLoading ? (
        <div className="team-projects__empty">{t('common.loading')}</div>
      ) : projects.length === 0 ? (
        <div className="team-projects__empty" data-testid="team-projects-empty">
          <p>{t('teamProjects.empty')}</p>
          <p className="team-projects__hint">{t('onboarding.teamProjectsHint')}</p>
          <div className="team-projects__empty-actions">
            <button
              type="button"
              className="team-projects__create-btn"
              data-testid="team-projects-create-first"
              disabled={!currentTeam}
              onClick={() => setProjectModalOpen(true)}
            >
              {t('onboarding.teamProjectsCreateFirst')}
            </button>
            {currentTeam && (
              <DemoDataButton
                teamId={currentTeam.id}
                teamName={currentTeam.name}
                existingPrefixes={existingPrefixes}
                onCreated={(result) => setCreatedPrefix(result.prefix)}
              />
            )}
          </div>
        </div>
      ) : (
        <ProjectsTable projects={projects} />
      )}

      {/* 削除確認ダイアログ */}
      {deleteConfirm !== null && (
        <div className="team-projects-dialog-overlay" onClick={() => !isDeleting && setDeleteConfirm(null)}>
          <div className="team-projects-dialog" onClick={(e) => e.stopPropagation()}>
            <h3 className="team-projects-dialog__title">{t('onboarding.deleteDemoConfirm')}</h3>
            <div className="team-projects-dialog__actions">
              <button
                className="team-projects-dialog__btn team-projects-dialog__btn--cancel"
                onClick={() => setDeleteConfirm(null)}
                disabled={isDeleting}
              >
                {t('common.cancel')}
              </button>
              <button
                className="team-projects-dialog__btn team-projects-dialog__btn--danger"
                onClick={() => void handleDeleteDemo(deleteConfirm)}
                disabled={isDeleting}
              >
                {t('common.delete')}
              </button>
            </div>
          </div>
        </div>
      )}

      {projectModalOpen && (
        <ProjectCreateModal
          onClose={() => setProjectModalOpen(false)}
          defaultTeamIds={currentTeam ? [currentTeam.id] : []}
          onCreated={() => {
            setProjectModalOpen(false);
          }}
        />
      )}
    </div>
  );
}
