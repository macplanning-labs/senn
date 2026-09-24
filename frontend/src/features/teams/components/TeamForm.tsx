/**
 * TeamForm.tsx — チームフォーム（共通コンポーネント）
 *
 * アイコン・カラー・名前・Prefix・説明・Webhook の入力フォーム。
 * TeamDetailModal と TeamGeneralSection から使用される。
 */

import { useTranslation } from 'react-i18next';

// 絵文字候補
const EMOJI_OPTIONS = ['👥', '🛠️', '🎨', '🔧', '📊', '🚀', '💻', '🔒', '📋', '⚙️', '🧪', '📦'];
const COLOR_OPTIONS = [
  '#6366f1', '#8b5cf6', '#ec4899', '#ef4444',
  '#f97316', '#f59e0b', '#22c55e', '#14b8a6',
  '#06b6d4', '#3b82f6', '#6b7280', '#0ea5e9',
];

interface TeamFormProps {
  name: string;
  prefix: string;
  description: string;
  icon: string;
  color: string;
  slackWebhookUrl: string;
  isActive: boolean;
  error: string;
  isPending: boolean;
  isEdit: boolean;
  onNameChange: (value: string) => void;
  onPrefixChange: (value: string) => void;
  onDescriptionChange: (value: string) => void;
  onIconChange: (value: string) => void;
  onColorChange: (value: string) => void;
  onWebhookChange: (value: string) => void;
  onActiveChange: (value: boolean) => void;
  onSubmit: (e: React.FormEvent) => void;
  submitLabel?: string;
  onCancel?: () => void;
  showActiveToggle?: boolean;
}

export function TeamForm({
  name,
  prefix,
  description,
  icon,
  color,
  slackWebhookUrl,
  isActive,
  error,
  isPending,
  isEdit,
  onNameChange,
  onPrefixChange,
  onDescriptionChange,
  onIconChange,
  onColorChange,
  onWebhookChange,
  onActiveChange,
  onSubmit,
  submitLabel,
  onCancel,
  showActiveToggle = true,
}: TeamFormProps) {
  const { t } = useTranslation();

  const validatePrefix = (value: string): string => {
    if (!value.trim()) {
      if (isEdit) {
        return t('team.modal.prefixRequired');
      }
      return '';
    }
    if (!/^[A-Z0-9]{1,20}$/.test(value)) {
      return t('team.modal.prefixInvalid');
    }
    return '';
  };

  const prefixError = validatePrefix(prefix);
  const hasError = error || prefixError;

  return (
    <form className="teams-modal__form" onSubmit={(e) => void onSubmit(e)}>
      {/* アイコン + カラー */}
      <div className="teams-modal__row">
        <div className="teams-modal__field">
          <label className="teams-modal__label">{t('team.modal.icon')}</label>
          <div className="teams-modal__emoji-grid">
            {EMOJI_OPTIONS.map((e) => (
              <button
                key={e}
                type="button"
                className={`teams-modal__emoji ${icon === e ? 'teams-modal__emoji--selected' : ''}`}
                onClick={() => onIconChange(e)}
              >
                {e}
              </button>
            ))}
          </div>
        </div>
        <div className="teams-modal__field">
          <label className="teams-modal__label">{t('team.modal.color')}</label>
          <div className="teams-modal__color-grid">
            {COLOR_OPTIONS.map((c) => (
              <button
                key={c}
                type="button"
                className={`teams-modal__color-swatch ${color === c ? 'teams-modal__color-swatch--selected' : ''}`}
                style={{ backgroundColor: c }}
                onClick={() => onColorChange(c)}
              />
            ))}
          </div>
        </div>
      </div>

      {/* チーム名 */}
      <div className="teams-modal__field">
        <label className="teams-modal__label">{t('team.modal.name')}</label>
        <input
          className="teams-modal__input"
          value={name}
          onChange={(e) => onNameChange(e.target.value)}
          placeholder={t('team.modal.namePlaceholder')}
          autoFocus
          data-autofocus="true"
          data-testid="team-name-input"
        />
      </div>

      {/* チーム Prefix */}
      <div className="teams-modal__field">
        <label className="teams-modal__label">{t('team.modal.prefix')} {isEdit && '*'}</label>
        <input
          className="teams-modal__input"
          value={prefix}
          onChange={(e) => onPrefixChange(e.target.value.toUpperCase())}
          placeholder={t('team.modal.prefixPlaceholder')}
          data-testid="team-prefix-input"
        />
        <div className="teams-modal__helper-text">
          {t('team.modal.prefixHelp')}
        </div>
      </div>

      {/* 説明 */}
      <div className="teams-modal__field">
        <label className="teams-modal__label">{t('team.modal.description')}</label>
        <textarea
          className="teams-modal__textarea"
          value={description}
          onChange={(e) => onDescriptionChange(e.target.value)}
          placeholder={t('team.descriptionPlaceholder')}
          rows={3}
        />
      </div>

      {/* 通知 Webhook */}
      <div className="teams-modal__field">
        <label className="teams-modal__label">{t('team.modal.webhook')}</label>
        <input
          className="teams-modal__input"
          type="url"
          value={slackWebhookUrl}
          onChange={(e) => onWebhookChange(e.target.value)}
          placeholder={t('team.webhookPlaceholder')}
        />
      </div>

      {/* 有効/無効 */}
      {showActiveToggle && isEdit && (
        <div className="teams-modal__field teams-modal__field--toggle">
          <label className="teams-modal__label">{t('team.modal.active')}</label>
          <button
            type="button"
            className={`teams-modal__toggle ${isActive ? 'teams-modal__toggle--on' : ''}`}
            onClick={() => onActiveChange(!isActive)}
          >
            <span className="teams-modal__toggle-knob" />
          </button>
        </div>
      )}

      {hasError && <div className="teams-modal__error">{hasError}</div>}

      <div className="teams-modal__footer">
        {onCancel && (
          <button
            type="button"
            className="teams-modal__btn teams-modal__btn--cancel"
            onClick={onCancel}
          >
            {t('team.modal.cancel')}
          </button>
        )}
        <button
          type="submit"
          className="teams-modal__btn teams-modal__btn--submit"
          disabled={isPending}
          data-testid="team-submit-btn"
        >
          {isPending ? t('team.modal.saving') : submitLabel || (isEdit ? t('team.modal.submitUpdate') : t('team.modal.submitCreate'))}
        </button>
      </div>
    </form>
  );
}
