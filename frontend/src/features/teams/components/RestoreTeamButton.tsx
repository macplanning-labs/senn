import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useUnarchiveTeam } from '../hooks/useTeams';
import { useToastStore } from '@/shared/stores/toastStore';
import './RestoreTeamButton.css';

interface RestoreTeamButtonProps {
  team: {
    id: number;
    name: string;
    viewerCanManage?: boolean;
    archivedAt?: string | null;
  };
}

export function RestoreTeamButton({ team }: RestoreTeamButtonProps) {
  const { t } = useTranslation();
  const { mutateAsync, isPending } = useUnarchiveTeam();
  const { addToast } = useToastStore();
  const [isConfirming, setIsConfirming] = useState(false);

  // フックの呼び出しは、条件付きリターンより前に
  // (既に呼んでいるので問題ない)

  // アーカイブ済みで、かつ管理者のみ表示
  if (!team.viewerCanManage || !team.archivedAt) {
    return null;
  }

  const handleRestore = async (e: React.MouseEvent<HTMLButtonElement>) => {
    e.preventDefault();
    e.stopPropagation();
    setIsConfirming(true);
  };

  const handleConfirm = async (e: React.MouseEvent<HTMLButtonElement>) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      await mutateAsync(team.id);
      addToast({ message: t('teamArchive.restored'), type: 'success' });
      setIsConfirming(false);
    } catch {
      // エラーはグローバル MutationCache が処理。ここでは確認を閉じずに待つ
    }
  };

  const handleCancel = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.preventDefault();
    e.stopPropagation();
    setIsConfirming(false);
  };

  if (isConfirming) {
    return (
      <div
        className="restore-team-btn__confirmation"
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <p className="restore-team-btn__confirmation-text">
          {t('teamArchive.restoreInlineConfirm')}
        </p>
        <div className="restore-team-btn__confirmation-actions">
          <button
            type="button"
            className="restore-team-btn__confirm-btn"
            onClick={handleConfirm}
            disabled={isPending}
            data-testid={`restore-team-confirm-${team.id}`}
          >
            {t('teamArchive.restoreYes')}
          </button>
          <button
            type="button"
            className="restore-team-btn__cancel-btn"
            onClick={handleCancel}
            disabled={isPending}
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
      onClick={handleRestore}
      disabled={isPending}
      data-testid={`restore-team-btn-${team.id}`}
    >
      {t('teamArchive.restoreInline')}
    </button>
  );
}
