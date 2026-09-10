/**
 * TeamDetailModal.tsx — チーム作成/編集モーダル
 *
 * マスタメンテナンス方式のモーダルフォーム。
 */

import { useState } from 'react';
import { useCreateTeam, useUpdateTeam, type TeamFormData } from '../hooks/useTeams';
import type { Team } from '@/shared/api/types';
import { useToastStore } from '@/shared/stores/toastStore';
import { useTranslation } from 'react-i18next';

// 絵文字候補
const EMOJI_OPTIONS = ['👥', '🛠️', '🎨', '🔧', '📊', '🚀', '💻', '🔒', '📋', '⚙️', '🧪', '📦'];
const COLOR_OPTIONS = [
  '#6366f1', '#8b5cf6', '#ec4899', '#ef4444',
  '#f97316', '#f59e0b', '#22c55e', '#14b8a6',
  '#06b6d4', '#3b82f6', '#6b7280', '#0ea5e9',
];

interface Props {
  team: Team | null; // null = 新規作成
  onClose: () => void;
}

export function TeamDetailModal({ team, onClose }: Props) {
  const { t } = useTranslation();
  const createTeam = useCreateTeam();
  const updateTeam = useUpdateTeam();
  const { addToast } = useToastStore();

  const [name, setName] = useState(team?.name ?? '');
  const [description, setDescription] = useState(team?.description ?? '');
  const [icon, setIcon] = useState(team?.icon ?? '👥');
  const [color, setColor] = useState(team?.color ?? '#6366f1');
  const [slackWebhookUrl, setSlackWebhookUrl] = useState(team?.slackWebhookUrl ?? '');
  const [isActive, setIsActive] = useState(team?.isActive ?? true);
  const [prefix, setPrefix] = useState(team?.prefix ?? '');
  const [error, setError] = useState('');
  const isEdit = team !== null;

  const validatePrefix = (value: string): string => {
    if (!value.trim()) {
      if (isEdit) {
        return 'Prefix は必須です';
      }
      return '';
    }
    if (!/^[A-Z0-9]{1,20}$/.test(value)) {
      return 'Prefix は英数字1〜20文字です';
    }
    return '';
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');

    if (!name.trim()) {
      setError('チーム名は必須です');
      return;
    }

    const prefixError = validatePrefix(prefix);
    if (prefixError) {
      setError(prefixError);
      return;
    }

    try {
      const trimmedPrefix = prefix.trim().toUpperCase();
      const data: TeamFormData = {
        name: name.trim(),
        description: description.trim(),
        icon,
        color,
        slackWebhookUrl: slackWebhookUrl.trim() || undefined,
        isActive: isActive,
        ...(trimmedPrefix ? { prefix: trimmedPrefix } : {}),
        ...(isEdit && team ? { slug: team.slug } : {}),
      };

      if (isEdit && team) {
        await updateTeam.mutateAsync({ id: team.id, data });
        addToast({ message: 'チームを更新しました', type: 'success' });
      } else {
        await createTeam.mutateAsync(data);
        addToast({ message: 'チームを作成しました', type: 'success' });
      }
      onClose();
    } catch {
      setError(isEdit ? '更新に失敗しました' : '作成に失敗しました');
    }
  };

  const isPending = createTeam.isPending || updateTeam.isPending;

  return (
    <div className="teams-modal-overlay" onClick={onClose}>
      <div className="teams-modal" onClick={(e) => e.stopPropagation()}>
        <div className="teams-modal__header">
          <h2 className="teams-modal__title">
            {isEdit ? 'チームを編集' : '新規チーム作成'}
          </h2>
          <button className="teams-modal__close" onClick={onClose}>✕</button>
        </div>

        <form className="teams-modal__form" onSubmit={(e) => void handleSubmit(e)}>
          {/* アイコン + カラー */}
          <div className="teams-modal__row">
            <div className="teams-modal__field">
              <label className="teams-modal__label">アイコン</label>
              <div className="teams-modal__emoji-grid">
                {EMOJI_OPTIONS.map((e) => (
                  <button
                    key={e}
                    type="button"
                    className={`teams-modal__emoji ${icon === e ? 'teams-modal__emoji--selected' : ''}`}
                    onClick={() => setIcon(e)}
                  >
                    {e}
                  </button>
                ))}
              </div>
            </div>
            <div className="teams-modal__field">
              <label className="teams-modal__label">カラー</label>
              <div className="teams-modal__color-grid">
                {COLOR_OPTIONS.map((c) => (
                  <button
                    key={c}
                    type="button"
                    className={`teams-modal__color-swatch ${color === c ? 'teams-modal__color-swatch--selected' : ''}`}
                    style={{ backgroundColor: c }}
                    onClick={() => setColor(c)}
                  />
                ))}
              </div>
            </div>
          </div>

          {/* チーム名 */}
          <div className="teams-modal__field">
            <label className="teams-modal__label">チーム名 *</label>
            <input
              className="teams-modal__input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="例: フロントエンドチーム"
              autoFocus
              data-testid="team-name-input"
            />
          </div>

          {/* チーム Prefix */}
          <div className="teams-modal__field">
            <label className="teams-modal__label">チーム Prefix {isEdit && '*'}</label>
            <input
              className="teams-modal__input"
              value={prefix}
              onChange={(e) => setPrefix(e.target.value.toUpperCase())}
              placeholder="例: DEMO"
              data-testid="team-prefix-input"
            />
            <div className="teams-modal__helper-text">
              新規チケットキーの先頭（例: DEMO-000001）。既存キーは変わりません。
            </div>
          </div>

          {/* スラッグはバックエンドで自動生成されるため非表示 */}

          {/* 説明 */}
          <div className="teams-modal__field">
            <label className="teams-modal__label">説明</label>
            <textarea
              className="teams-modal__textarea"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder={t("team.descriptionPlaceholder")}
              rows={3}
            />
          </div>

          {/* 通知 Webhook */}
          <div className="teams-modal__field">
            <label className="teams-modal__label">通知 Webhook URL</label>
            <input
              className="teams-modal__input"
              type="url"
              value={slackWebhookUrl}
              onChange={(e) => setSlackWebhookUrl(e.target.value)}
              placeholder={t("team.webhookPlaceholder")}
            />
          </div>

          {/* 有効/無効 */}
          {isEdit && (
            <div className="teams-modal__field teams-modal__field--toggle">
              <label className="teams-modal__label">有効</label>
              <button
                type="button"
                className={`teams-modal__toggle ${isActive ? 'teams-modal__toggle--on' : ''}`}
                onClick={() => setIsActive(!isActive)}
              >
                <span className="teams-modal__toggle-knob" />
              </button>
            </div>
          )}

          {error && <div className="teams-modal__error">{error}</div>}

          <div className="teams-modal__footer">
            <button
              type="button"
              className="teams-modal__btn teams-modal__btn--cancel"
              onClick={onClose}
            >
              キャンセル
            </button>
            <button
              type="submit"
              className="teams-modal__btn teams-modal__btn--submit"
              disabled={isPending}
              data-testid="team-submit-btn"
            >
              {isPending ? '保存中...' : isEdit ? '更新' : '作成'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
