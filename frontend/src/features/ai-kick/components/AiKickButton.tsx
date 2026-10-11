/**
 * AiKickButton.tsx - AI Kick ボタンコンポーネント
 *
 * チケット詳細ヘッダーに配置するボタン
 */

import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { canShowAiKick, isActive } from '../format';
import { AiKickDialog } from './AiKickDialog';
import type { ProjectAiKick, AiKickRun } from '../api';
import '../i18n';

interface AiKickButtonProps {
  ticketKey: string;
  projectId: number | null;
  projectKick: ProjectAiKick | null;
  isGuest: boolean;
  onRun?: (run: AiKickRun) => void;
  /** icon: ヘッダーのアイコン(既定) / subbar: サブバーの文字つきボタン */
  variant?: 'icon' | 'subbar';
}

export function AiKickButton({
  ticketKey,
  projectId,
  projectKick,
  isGuest,
  onRun,
  variant = 'icon',
}: AiKickButtonProps) {
  const { t } = useTranslation();
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [activeRun, setActiveRun] = useState<AiKickRun | null>(null);

  // 親コンポーネントから activeRun が渡された場合は使用
  useEffect(() => {
    if (onRun && activeRun) {
      onRun(activeRun);
    }
  }, [activeRun, onRun]);

  // ボタンを表示すべきか判定
  if (!canShowAiKick({ projectKick: projectKick ?? undefined, isGuest: isGuest ?? false })) {
    return null;
  }

  // ボタンが無効になる条件: systemKilled か projectId がない
  const isDisabled = ((projectKick?.systemKilled) ?? false) || !projectId;
  const isLoading = !!(activeRun && isActive(activeRun));

  return (
    <>
      <button
        type="button"
        className={variant === 'subbar' ? 'ticket-detail-subbar__button' : 'detail-panel__edit-btn'}
        onClick={() => setIsDialogOpen(true)}
        disabled={isDisabled || isLoading}
        aria-label={t('aiKick.button.label')}
        title={
          isDisabled
            ? t('aiKick.systemKilled')
            : isLoading
              ? t('aiKick.status.running')
              : t('aiKick.button.tooltip')
        }
        data-testid="ai-kick-btn"
      >
        {variant === 'subbar'
          ? `${isLoading ? '⏳' : '🤖'} ${t('aiKick.button.label')}`
          : isLoading ? '⏳' : '🤖'}
      </button>

      {isDialogOpen && projectId && (
        <AiKickDialog
          ticketKey={ticketKey}
          onClose={() => setIsDialogOpen(false)}
          onRunCreated={(run) => {
            setActiveRun(run);
            setIsDialogOpen(false);
          }}
        />
      )}
    </>
  );
}
