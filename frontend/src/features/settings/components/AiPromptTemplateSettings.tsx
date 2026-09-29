/**
 * AiPromptTemplateSettings.tsx - ユーザー個人設定のAIプロンプト設定
 *
 * 共通・Cursor用・Claude Code用の3つのテンプレートを管理
 * 未設定(null)のときは既定値を表示、ユーザーが保存を押したときだけ保存
 */

import { useTranslation } from 'react-i18next';
import { useAiPromptTemplates } from '../hooks/useAiPromptTemplates';
import { AiPromptTemplateEditor } from './AiPromptTemplateEditor';

export function AiPromptTemplateSettings() {
  const { t } = useTranslation();
  const { templates, isLoading, save, isSaving } = useAiPromptTemplates();

  if (isLoading) {
    return null;
  }

  const defaults = {
    common: t('settings.defaultAiPromptTemplate'),
    cursor: t('settings.defaultAiPromptTemplateCursor'),
    claude: t('settings.defaultAiPromptTemplateClaude'),
  };

  const currentValues = {
    common: templates?.common || defaults.common,
    cursor: templates?.cursor || defaults.cursor,
    claude: templates?.claude || defaults.claude,
  };

  return (
    <section className="settings__section">
      <h2 className="settings__section-title">🤖 {t('settings.aiPromptTemplateTitle')}</h2>
      <div className="settings__card">
        <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)', marginBottom: 'var(--space-4)' }}>
          {t('settings.aiPromptTemplateDesc')}
        </div>

        <AiPromptTemplateEditor
          label={t('settings.aiPromptTemplateTitle')}
          value={currentValues.common}
          defaultValue={defaults.common}
          onChange={(value) => {
            save({
              common: value?.trim() ? value.trim() : null,
              cursor: templates?.cursor ?? null,
              claude: templates?.claude ?? null,
            });
          }}
          isSaving={isSaving}
          isCustom={!!templates?.common}
        />

        <hr style={{ margin: 'var(--space-4) 0', borderColor: 'var(--color-border-default)' }} />

        <AiPromptTemplateEditor
          label={t('settings.aiPromptTemplateCursorTitle')}
          value={currentValues.cursor}
          defaultValue={defaults.cursor}
          onChange={(value) => {
            save({
              common: templates?.common ?? null,
              cursor: value?.trim() ? value.trim() : null,
              claude: templates?.claude ?? null,
            });
          }}
          isSaving={isSaving}
          isCustom={!!templates?.cursor}
        />

        <hr style={{ margin: 'var(--space-4) 0', borderColor: 'var(--color-border-default)' }} />

        <AiPromptTemplateEditor
          label={t('settings.aiPromptTemplateClaudeTitle')}
          value={currentValues.claude}
          defaultValue={defaults.claude}
          onChange={(value) => {
            save({
              common: templates?.common ?? null,
              cursor: templates?.cursor ?? null,
              claude: value?.trim() ? value.trim() : null,
            });
          }}
          isSaving={isSaving}
          isCustom={!!templates?.claude}
        />
      </div>
    </section>
  );
}
