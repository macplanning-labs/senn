/**
 * DemoDataButton.tsx — サンプルデータ生成ボタン
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQueryClient } from '@tanstack/react-query';
import { createDemoData } from '../demoData';
import { useToast } from '@/shared/stores/toastStore';

interface Props {
  teamId: number;
  teamName: string;
  existingPrefixes: string[];
  onCreated: (result: { prefix: string }) => void;
  /** true なら tryDemoForTeam を使う、false なら tryDemo を使う */
  withTeamName?: boolean;
}

export function DemoDataButton({
  teamId,
  teamName,
  existingPrefixes,
  onCreated,
  withTeamName = false,
}: Props) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { error: showError } = useToast();

  const [isLoading, setIsLoading] = useState(false);

  const handleCreate = async () => {
    if (isLoading) return;

    setIsLoading(true);
    try {
      const result = await createDemoData(teamId, existingPrefixes);

      // キャッシュを無効化
      void queryClient.invalidateQueries({ queryKey: ['projects'] });
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
      void queryClient.invalidateQueries({ queryKey: ['teams'] });

      // 呼び出し側に通知（完了メッセージ表示用）
      onCreated({ prefix: result.prefix });
    } catch (err) {
      showError(t('onboarding.createFailed'));
      console.error('Demo data creation failed:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const label = withTeamName
    ? t('onboarding.tryDemoForTeam', { team: teamName })
    : t('onboarding.tryDemo');

  return (
    <div className="demo-data-button">
      <button
        type="button"
        data-testid="demo-data-btn"
        onClick={handleCreate}
        disabled={isLoading}
        className="demo-data-button__btn"
      >
        {isLoading ? t('onboarding.creating') : label}
      </button>
      <p className="demo-data-button__hint">{t('onboarding.tryDemoHint')}</p>
    </div>
  );
}
