/**
 * GettingStartedChecklist.tsx — オンボーディング チェックリスト
 *
 * チーム・プロジェクト作成の進捗を表示。STEP2 で使用。
 * STEP3 でサンプルデータ生成ボタンと完了メッセージを追加。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useTeam, getLastTeamSlug } from '@/shared/hooks/useTeam';
import { useProject } from '@/shared/hooks/useProject';
import { useUIStore } from '@/shared/stores/uiStore';
import { ProjectCreateModal } from '@/features/projects/components/ProjectCreateModal';
import { DemoDataButton } from './DemoDataButton';
import { DemoReadyMessage } from './DemoReadyMessage';
import './GettingStartedChecklist.css';

interface Props {
  /** サンプル生成済みのプロジェクト prefix。親（MyIssuesPage）が保持する。
   *  生成直後は hasProject が真になり親の分岐が切り替わるため、状態を親に持たせて完了メッセージを残す */
  createdPrefix: string | null;
  onDemoCreated: (prefix: string) => void;
}

export function GettingStartedChecklist({ createdPrefix, onDemoCreated }: Props) {
  const { t } = useTranslation();
  const { teamList, isLoading: teamsLoading } = useTeam();
  const { projectList, isLoading: projectsLoading } = useProject();
  const { openTicketFormModal } = useUIStore();

  const [projectModalOpen, setProjectModalOpen] = useState(false);

  const hasTeam = teamList.length > 0;
  // チームが1つだけのときは、プロジェクトの参加チームとして初期選択する
  const singleTeamId = teamList.length === 1 ? teamList[0]?.id : undefined;

  // ローディング中は何も描画しない
  if (teamsLoading || projectsLoading) {
    return null;
  }

  // サンプル生成完了メッセージ（自動遷移はせず、ユーザーが自分でリンクを押す）
  if (createdPrefix) {
    return (
      <div className="getting-started-checklist" data-testid="getting-started-checklist">
        <DemoReadyMessage prefix={createdPrefix} />
      </div>
    );
  }

  // チーム作成用：DemoDataButton・チケット作成用に必要なチーム情報を決定
  let demoTeamId: number | null = null;
  let demoTeamName = '';
  let demoTeamSlug = '';

  if (hasTeam) {
    const lastTeamSlug = getLastTeamSlug();
    const lastTeam = lastTeamSlug
      ? teamList.find((t) => t.slug.toLowerCase() === lastTeamSlug.toLowerCase())
      : null;
    const selectedTeam = lastTeam ?? teamList[0];
    if (selectedTeam) {
      demoTeamId = selectedTeam.id;
      demoTeamName = selectedTeam.name;
      demoTeamSlug = selectedTeam.slug;
    }
  }

  const existingPrefixes = projectList.map((p) => p.prefix);

  return (
    <div className="getting-started-checklist" data-testid="getting-started-checklist">
      <h3 className="getting-started-checklist__title">{t('onboarding.checklistTitle')}</h3>

      <div className="getting-started-checklist__steps">
        {/* Step 1: チームを作る */}
        <div className="getting-started-checklist__step">
          <div className="getting-started-checklist__step-header">
            {hasTeam ? (
              <div className="getting-started-checklist__checkmark">✓</div>
            ) : (
              <div className="getting-started-checklist__number">1</div>
            )}
            <span className="getting-started-checklist__step-title">{t('onboarding.stepTeam')}</span>
            {hasTeam && (
              <span className="getting-started-checklist__done-badge">{t('onboarding.stepDone')}</span>
            )}
          </div>
          {!hasTeam && (
            <p className="getting-started-checklist__hint">{t('onboarding.stepTeamHint')}</p>
          )}
        </div>

        {/* Step 2: 次にやることを選ぶ */}
        <div className={`getting-started-checklist__step ${!hasTeam ? 'getting-started-checklist__step--disabled' : ''}`}>
          <div className="getting-started-checklist__step-header">
            <div className="getting-started-checklist__number">2</div>
            <span className="getting-started-checklist__step-title">{t('onboarding.stepChoose')}</span>
          </div>
          {!hasTeam ? (
            <p className="getting-started-checklist__hint">{t('onboarding.stepChooseLocked')}</p>
          ) : (
            <>
              {/* 2つの選択肢を横並び */}
              <div className="getting-started-checklist__options">
                {/* 選択肢A: チケットをすぐに作る */}
                <div className="getting-started-checklist__option" data-testid="checklist-option-ticket">
                  <h4 className="getting-started-checklist__option-title">
                    {t('onboarding.optionTicketTitle')}
                  </h4>
                  <p className="getting-started-checklist__option-hint">
                    {t('onboarding.optionTicketHint')}
                  </p>
                  <button
                    type="button"
                    className="getting-started-checklist__create-btn"
                    data-testid="checklist-create-ticket"
                    onClick={() => openTicketFormModal(null, demoTeamSlug)}
                  >
                    {t('onboarding.createTicket')}
                  </button>
                </div>

                {/* 選択肢B: プロジェクトを作る */}
                <div className="getting-started-checklist__option" data-testid="checklist-option-project">
                  <h4 className="getting-started-checklist__option-title">
                    {t('onboarding.optionProjectTitle')}
                  </h4>
                  <p className="getting-started-checklist__option-hint">
                    {t('onboarding.optionProjectHint')}
                  </p>
                  <button
                    type="button"
                    className="getting-started-checklist__create-btn"
                    data-testid="checklist-create-project"
                    onClick={() => setProjectModalOpen(true)}
                  >
                    {t('onboarding.createProject')}
                  </button>
                </div>
              </div>

              {/* STEP3: サンプルデータを試す（オプション） */}
              {demoTeamId && (
                <div className="getting-started-checklist__option getting-started-checklist__option--demo" data-testid="checklist-option-demo">
                  <h4 className="getting-started-checklist__option-title">
                    {t('onboarding.optionDemoTitle')}
                  </h4>
                  <DemoDataButton
                    teamId={demoTeamId}
                    teamName={demoTeamName}
                    existingPrefixes={existingPrefixes}
                    onCreated={(result) => onDemoCreated(result.prefix)}
                    withTeamName
                  />
                </div>
              )}
            </>
          )}
        </div>
      </div>

      {projectModalOpen && (
        <ProjectCreateModal
          onClose={() => setProjectModalOpen(false)}
          defaultTeamIds={singleTeamId !== undefined ? [singleTeamId] : []}
          onCreated={() => {
            setProjectModalOpen(false);
            // トースト不要（設計書 §5.1(a)）
          }}
        />
      )}
    </div>
  );
}
