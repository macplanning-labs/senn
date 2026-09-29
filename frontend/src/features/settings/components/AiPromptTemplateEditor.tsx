/**
 * AiPromptTemplateEditor.tsx - AIプロンプト設定の編集欄(共通コンポーネント)
 *
 * 表示→編集→保存/キャンセル/既定値に戻す パターン
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';

interface AiPromptTemplateEditorProps {
  label: string;
  value: string;
  defaultValue: string;
  onChange: (value: string) => void;
  isSaving: boolean;
  isCustom: boolean;
}

export function AiPromptTemplateEditor({
  label,
  value,
  defaultValue,
  onChange,
  isSaving,
  isCustom,
}: AiPromptTemplateEditorProps) {
  const { t } = useTranslation();
  const [isEditing, setIsEditing] = useState(false);
  const [editValue, setEditValue] = useState(value);

  const handleSave = () => {
    onChange(editValue);
    setIsEditing(false);
  };

  const handleCancel = () => {
    setEditValue(value);
    setIsEditing(false);
  };

  const handleReset = () => {
    onChange('');
    setIsEditing(false);
  };

  return (
    <div style={{ marginBottom: 'var(--space-4)' }}>
      <div style={{ marginBottom: 'var(--space-2)' }}>
        <div style={{ fontWeight: 'var(--font-weight-semibold)', color: 'var(--color-text-primary)', marginBottom: 'var(--space-1)' }}>
          {label}
        </div>
        {!isEditing && (
          <div style={{ fontSize: 'var(--font-size-sm)', color: 'var(--color-text-tertiary)' }}>
            {isCustom ? '✏️ ' : ''}
            {t('settings.aiPromptTemplateDesc')}
          </div>
        )}
      </div>

      {!isEditing ? (
        <div
          onClick={() => {
            setEditValue(value);
            setIsEditing(true);
          }}
          style={{
            padding: 'var(--space-3)',
            background: 'var(--color-bg-elevated)',
            border: '1px solid var(--color-border-default)',
            borderRadius: 'var(--radius-md)',
            color: 'var(--color-text-primary)',
            fontSize: 'var(--font-size-sm)',
            minHeight: 80,
            cursor: 'pointer',
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
          }}
        >
          {value || defaultValue}
        </div>
      ) : (
        <textarea
          value={editValue}
          onChange={(e) => setEditValue(e.target.value)}
          style={{
            width: '100%',
            padding: 'var(--space-3)',
            border: '1px solid var(--color-border-default)',
            borderRadius: 'var(--radius-md)',
            fontFamily: 'monospace',
            fontSize: 'var(--font-size-sm)',
            minHeight: 120,
            resize: 'vertical',
          }}
          disabled={isSaving}
        />
      )}

      {isEditing && (
        <div style={{ display: 'flex', gap: 'var(--space-2)', marginTop: 'var(--space-3)' }}>
          <button
            type="button"
            className="settings__btn settings__btn--primary"
            disabled={isSaving || editValue === value}
            onClick={handleSave}
          >
            {t('common.save', 'Save')}
          </button>
          <button
            type="button"
            className="settings__btn settings__btn--secondary"
            disabled={isSaving}
            onClick={handleCancel}
          >
            {t('common.cancel', 'Cancel')}
          </button>
          {isCustom && (
            <button
              type="button"
              className="settings__btn settings__btn--secondary"
              disabled={isSaving}
              onClick={handleReset}
            >
              {t('settings.aiPromptTemplateReset')}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
