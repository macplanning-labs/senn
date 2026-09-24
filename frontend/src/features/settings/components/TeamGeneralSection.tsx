/**
 * TeamGeneralSection.tsx — チーム設定の一般タブ
 *
 * チーム情報の編集（名前・アイコン・色等）と削除機能。
 * TeamSettingsPage の general タブで表示される。
 */

import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import {
  useUpdateTeam,
  useDeleteTeam,
  useArchiveTeam,
  useUnarchiveTeam,
  useTeamMembers,
  useCheckTeamArchive,
  type BlockingProject,
} from '@/features/teams/hooks/useTeams';
import { TeamForm } from '@/features/teams/components/TeamForm';
import { buildTeamUpdateData } from '../utils/teamUpdatePayload';
import { projectStatusLabelKey } from '../utils/teamArchiveCheck';
import { canManageTeam } from '@/features/teams/utils/teamPermissions';
import { useAuthStore } from '@/shared/stores/authStore';
import { useToastStore } from '@/shared/stores/toastStore';
import type { Team } from '@/shared/api/types';
import './TeamGeneralSection.css';

interface Props {
  team: Team;
}

export function TeamGeneralSection({ team }: Props) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const updateTeam = useUpdateTeam();
  const deleteTeam = useDeleteTeam();
  const archiveTeam = useArchiveTeam();
  const unarchiveTeam = useUnarchiveTeam();
  const checkTeamArchive = useCheckTeamArchive();
  const { addToast } = useToastStore();
  const viewer = useAuthStore((st) => st.user);
  const { data: members } = useTeamMembers(team.id);
  // アーカイブ・復元ボタンは、できる人(システム管理者 / チームの管理者)にだけ出す。最終判定はサーバー
  const canArchive = canManageTeam(viewer, members);

  const [name, setName] = useState(team?.name ?? '');
  const [description, setDescription] = useState(team?.description ?? '');
  const [icon, setIcon] = useState(team?.icon ?? '👥');
  const [color, setColor] = useState(team?.color ?? '#6366f1');
  const [slackWebhookUrl, setSlackWebhookUrl] = useState(team?.slackWebhookUrl ?? '');
  const [prefix, setPrefix] = useState(team?.prefix ?? '');
  const [error, setError] = useState('');
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [showArchiveConfirm, setShowArchiveConfirm] = useState(false);
  const [isArchiving, setIsArchiving] = useState(false);
  const [showRestoreConfirm, setShowRestoreConfirm] = useState(false);
  const [isRestoring, setIsRestoring] = useState(false);
  const [blockedProjects, setBlockedProjects] = useState<BlockingProject[] | null>(null);

  const isTeamArchived = !!team?.archivedAt;

  const validatePrefix = (value: string): string => {
    if (!value.trim()) {
      return t('team.modal.prefixRequired');
    }
    if (!/^[A-Z0-9]{1,20}$/.test(value)) {
      return t('team.modal.prefixInvalid');
    }
    return '';
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');

    if (!name.trim()) {
      setError(t('team.modal.nameRequired'));
      return;
    }

    const prefixError = validatePrefix(prefix);
    if (prefixError) {
      setError(prefixError);
      return;
    }

    try {
      const data = buildTeamUpdateData({
        name,
        description,
        icon,
        color,
        slackWebhookUrl,
        prefix,
        isActive: team.isActive,
      });

      await updateTeam.mutateAsync({ id: team.id, data });
      addToast({ message: t('team.modal.updated'), type: 'success' });
    } catch {
      setError(t('team.modal.updateFailed'));
    }
  };

  const handleArchiveClick = async () => {
    // パネルが開いていたら閉じる(トグル)
    if (showArchiveConfirm || blockedProjects !== null) {
      setShowArchiveConfirm(false);
      setBlockedProjects(null);
      return;
    }

    // パネルが閉じていたら、アーカイブ可能かチェックする
    try {
      const result = await checkTeamArchive.mutateAsync(team.id);
      if (result.canArchive) {
        // アーカイブ可能：確認欄を出す
        setShowArchiveConfirm(true);
        setBlockedProjects(null);
      } else {
        // アーカイブ不可：ブロッキングプロジェクトパネルを出す
        setBlockedProjects(result.blockingProjects);
        setShowArchiveConfirm(false);
      }
    } catch {
      // API失敗時は確認欄を出す(サーバー側の archive がする従来動作にフォールバック)
      setShowArchiveConfirm(true);
      setBlockedProjects(null);
    }
  };

  const handleArchiveConfirm = async () => {
    setIsArchiving(true);
    try {
      await archiveTeam.mutateAsync(team.id);
      addToast({ message: t('teamArchive.archived'), type: 'success' });
      setShowArchiveConfirm(false);
    } catch {
      setIsArchiving(false);
    }
  };

  const handleRestoreConfirm = async () => {
    setIsRestoring(true);
    try {
      await unarchiveTeam.mutateAsync(team.id);
      addToast({ message: t('teamArchive.restored'), type: 'success' });
      setShowRestoreConfirm(false);
    } catch {
      setIsRestoring(false);
    }
  };

  const handleDeleteConfirm = async () => {
    setIsDeleting(true);
    try {
      await deleteTeam.mutateAsync(team.id);
      addToast({ message: t('team.modal.deleted'), type: 'success' });
      setShowDeleteConfirm(false);
      // 削除成功後は /teams へ遷移（ユーザー操作の結果としての遷移）
      navigate('/teams');
    } catch {
      // 失敗の理由(例: 紐づくプロジェクトがある)は、App.tsx のグローバル MutationCache が
      // サーバーの detail をトースト表示する。ここで固定文言を重ねて出すと二重になる。
      setIsDeleting(false);
    }
  };

  return (
    <div className="team-general-section">
      <div className="team-general-section__content">
        {/* アーカイブ済みバナー */}
        {isTeamArchived && team.archivedAt && (
          <div className="team-general-section__archive-banner">
            <p className="team-general-section__archive-banner-text">
              {t('teamArchive.archivedBanner')}{' '}
              {t('teamArchive.archivedOn', {
                date: new Date(team.archivedAt).toLocaleDateString(),
              })}
            </p>
          </div>
        )}

        <fieldset disabled={isTeamArchived} className="team-general-section__fieldset">
          <div className="team-general-section__form-container">
            <h3 className="team-general-section__heading">{t('team.general')}</h3>
            <TeamForm
              name={name}
              prefix={prefix}
              description={description}
              icon={icon}
              color={color}
              slackWebhookUrl={slackWebhookUrl}
              isActive={true}
              error={error}
              isPending={updateTeam.isPending}
              isEdit={true}
              onNameChange={setName}
              onPrefixChange={setPrefix}
              onDescriptionChange={setDescription}
              onIconChange={setIcon}
              onColorChange={setColor}
              onWebhookChange={setSlackWebhookUrl}
              onActiveChange={() => {}}
              onSubmit={handleSubmit}
              submitLabel={updateTeam.isPending ? t('team.modal.saving') : t('team.modal.submitUpdate')}
              showActiveToggle={false}
            />
          </div>

          {isTeamArchived && (
            <div className="team-general-section__settings-locked">
              <p>{t('teamArchive.settingsLocked')}</p>
            </div>
          )}
        </fieldset>

        {/* アーカイブセクション */}
        <div className="team-general-section__archive-section">
          <h3 className="team-general-section__archive-heading">{t('teamArchive.title')}</h3>
          <p className="team-general-section__archive-description">
            {t('teamArchive.description')}
          </p>

          {!canArchive ? (
            <p className="team-general-section__archive-description" data-testid="team-archive-admin-only">
              {t('teamArchive.adminOnly')}
            </p>
          ) : !isTeamArchived ? (
            <>
              <button
                className="team-general-section__archive-btn"
                onClick={() => void handleArchiveClick()}
                type="button"
                data-testid="team-archive-btn"
                disabled={checkTeamArchive.isPending}
              >
                {checkTeamArchive.isPending ? t('team.modal.saving') : t('teamArchive.archive')}
              </button>

              {showArchiveConfirm && (
                <div className="team-general-section__inline-confirm">
                  <p className="team-general-section__confirm-message">
                    {t('teamArchive.archiveConfirm')}
                  </p>
                  <div className="team-general-section__confirm-actions">
                    <button
                      className="team-general-section__confirm-btn team-general-section__confirm-btn--cancel"
                      onClick={() => setShowArchiveConfirm(false)}
                      disabled={isArchiving}
                    >
                      {t('teamArchive.cancel')}
                    </button>
                    <button
                      className="team-general-section__confirm-btn team-general-section__confirm-btn--danger"
                      onClick={() => void handleArchiveConfirm()}
                      disabled={isArchiving}
                      data-testid="team-archive-confirm"
                    >
                      {isArchiving ? t('team.modal.saving') : t('teamArchive.archiveYes')}
                    </button>
                  </div>
                </div>
              )}

              {blockedProjects !== null && (
                <div className="team-general-section__blocked" data-testid="team-archive-blocked">
                  <h4 className="team-general-section__blocked-title">
                    {t('teamArchive.blockedTitle')}
                  </h4>
                  <p className="team-general-section__blocked-reason">
                    {t('teamArchive.blockedReason')}
                  </p>
                  <ul className="team-general-section__blocked-list">
                    {blockedProjects.slice(0, 5).map((project) => {
                      const statusKey = projectStatusLabelKey(project.status);
                      const statusLabel = statusKey ? t(statusKey) : project.status;
                      return (
                        <li
                          key={project.id}
                          data-testid="team-archive-blocked-project"
                        >
                          {project.name}({project.prefix}) {statusLabel}
                        </li>
                      );
                    })}
                  </ul>
                  {blockedProjects.length > 5 && (
                    <p className="team-general-section__blocked-more">
                      {t('teamArchive.blockedMore', { count: blockedProjects.length - 5 })}
                    </p>
                  )}
                  <p className="team-general-section__blocked-remedy">
                    {t('teamArchive.blockedRemedy')}
                  </p>
                  <p className="team-general-section__blocked-note">
                    {t('teamArchive.blockedTicketNote')}
                  </p>
                  <button
                    className="team-general-section__blocked-close"
                    onClick={() => void handleArchiveClick()}
                    type="button"
                  >
                    {t('teamArchive.close')}
                  </button>
                </div>
              )}
            </>
          ) : (
            <>
              <button
                className="team-general-section__restore-btn"
                onClick={() => setShowRestoreConfirm(true)}
                type="button"
                data-testid="team-restore-btn"
              >
                {t('teamArchive.restore')}
              </button>

              {showRestoreConfirm && (
                <div className="team-general-section__inline-confirm">
                  <p className="team-general-section__confirm-message">
                    {t('teamArchive.restoreConfirm')}
                  </p>
                  <div className="team-general-section__confirm-actions">
                    <button
                      className="team-general-section__confirm-btn team-general-section__confirm-btn--cancel"
                      onClick={() => setShowRestoreConfirm(false)}
                      disabled={isRestoring}
                    >
                      {t('teamArchive.cancel')}
                    </button>
                    <button
                      className="team-general-section__confirm-btn team-general-section__confirm-btn--primary"
                      onClick={() => void handleRestoreConfirm()}
                      disabled={isRestoring}
                      data-testid="team-restore-confirm"
                    >
                      {isRestoring ? t('team.modal.saving') : t('teamArchive.restoreYes')}
                    </button>
                  </div>
                </div>
              )}
            </>
          )}
        </div>

        {/* 危険な操作エリア */}
        <div className="team-general-section__danger-zone">
          <h3 className="team-general-section__danger-heading">{t('team.dangerZone')}</h3>
          <p className="team-general-section__danger-description">
            {t('team.deleteTeamWarning')}
          </p>
          <button
            className="team-general-section__danger-btn"
            onClick={() => setShowDeleteConfirm(true)}
            type="button"
          >
            {t('team.delete')}
          </button>
        </div>
      </div>

      {/* 削除確認ダイアログ */}
      {showDeleteConfirm && (
        <div
          className="team-general-section__dialog-overlay"
          onClick={() => !isDeleting && setShowDeleteConfirm(false)}
        >
          <div
            className="team-general-section__dialog"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="team-general-section__dialog-title">
              {t('team.deleteConfirm')}
            </h3>
            <p className="team-general-section__dialog-message">
              {t('team.deleteTeamWarning')}
            </p>
            <div className="team-general-section__dialog-actions">
              <button
                className="team-general-section__dialog-btn team-general-section__dialog-btn--cancel"
                onClick={() => setShowDeleteConfirm(false)}
                disabled={isDeleting}
              >
                {t('team.modal.cancel')}
              </button>
              <button
                className="team-general-section__dialog-btn team-general-section__dialog-btn--danger"
                onClick={() => void handleDeleteConfirm()}
                disabled={isDeleting}
                data-testid="confirm-delete-team"
              >
                {isDeleting ? t('team.modal.saving') : t('team.delete')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
