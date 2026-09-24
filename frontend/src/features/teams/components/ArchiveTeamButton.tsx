/**
 * ArchiveTeamButton — チーム一覧行のアーカイブ（管理者のみ）
 *
 * RestoreTeamButton と同型。アーカイブ前に archive-check を行い、
 * ブロック時はプロジェクト名を短く表示する。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useArchiveTeam, useCheckTeamArchive } from '../hooks/useTeams';
import { useToastStore } from '@/shared/stores/toastStore';
import './RestoreTeamButton.css';

interface ArchiveTeamButtonProps {
  team: {
    id: number;
    name: string;
    viewerCanManage?: boolean;
    archivedAt?: string | null;
  };
}

export function ArchiveTeamButton({ team }: ArchiveTeamButtonProps) {
  const { t } = useTranslation();
  const archiveTeam = useArchiveTeam();
  const checkTeamArchive = useCheckTeamArchive();
  const { addToast } = useToastStore();
  const [phase, setPhase] = useState<'idle' | 'confirm' | 'blocked'>('idle');
  const [blockedNames, setBlockedNames] = useState<string[]>([]);

  if (!team.viewerCanManage || team.archivedAt) {
    return null;
  }

  const stop = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };

  const handleStart = async (e: React.MouseEvent<HTMLButtonElement>) => {
    stop(e);
    if (phase !== 'idle') {
      setPhase('idle');
      setBlockedNames([]);
      return;
    }
    try {
      const result = await checkTeamArchive.mutateAsync(team.id);
      if (result.canArchive) {
        setPhase('confirm');
        setBlockedNames([]);
      } else {
        setBlockedNames(
          (result.blockingProjects ?? []).slice(0, 5).map((p) => p.name || p.prefix || String(p.id)),
        );
        setPhase('blocked');
      }
    } catch {
      setPhase('confirm');
      setBlockedNames([]);
    }
  };

  const handleConfirm = async (e: React.MouseEvent<HTMLButtonElement>) => {
    stop(e);
    try {
      await archiveTeam.mutateAsync(team.id);
      addToast({ message: t('teamArchive.archived'), type: 'success' });
      setPhase('idle');
    } catch {
      /* MutationCache が処理 */
    }
  };

  const handleCancel = (e: React.MouseEvent<HTMLButtonElement>) => {
    stop(e);
    setPhase('idle');
    setBlockedNames([]);
  };

  const pending = archiveTeam.isPending || checkTeamArchive.isPending;

  if (phase === 'blocked') {
    return (
      <div className="restore-team-btn__confirmation" onClick={stop}>
        <p className="restore-team-btn__confirmation-text">{t('teamArchive.blockedTitle')}</p>
        {blockedNames.length > 0 && (
          <ul className="restore-team-btn__blocked-list">
            {blockedNames.map((name) => (
              <li key={name}>{name}</li>
            ))}
          </ul>
        )}
        <p className="restore-team-btn__confirmation-text">{t('teamArchive.blockedRemedy')}</p>
        <button
          type="button"
          className="restore-team-btn__cancel-btn"
          onClick={handleCancel}
          data-testid={`archive-team-blocked-close-${team.id}`}
        >
          {t('teamArchive.close')}
        </button>
      </div>
    );
  }

  if (phase === 'confirm') {
    return (
      <div className="restore-team-btn__confirmation" onClick={stop}>
        <p className="restore-team-btn__confirmation-text">{t('teamArchive.archiveConfirm')}</p>
        <div className="restore-team-btn__confirmation-actions">
          <button
            type="button"
            className="restore-team-btn__confirm-btn"
            onClick={handleConfirm}
            disabled={pending}
            data-testid={`archive-team-confirm-${team.id}`}
          >
            {t('teamArchive.archiveYes')}
          </button>
          <button
            type="button"
            className="restore-team-btn__cancel-btn"
            onClick={handleCancel}
            disabled={pending}
          >
            {t('teamArchive.cancel')}
          </button>
        </div>
      </div>
    );
  }

  return (
    <button
      type="button"
      className="restore-team-btn"
      onClick={handleStart}
      disabled={pending}
      data-testid={`archive-team-btn-${team.id}`}
    >
      {t('teamArchive.archive')}
    </button>
  );
}
