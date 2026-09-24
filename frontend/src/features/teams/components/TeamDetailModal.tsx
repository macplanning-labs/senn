/**
 * TeamDetailModal.tsx — チーム作成/編集モーダル
 *
 * 作成・編集とも TeamForm を使う。ポータル＋フォーカストラップ。
 */

import { useState, useRef, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useCreateTeam, useUpdateTeam } from '../hooks/useTeams';
import type { Team } from '@/shared/api/types';
import { useToastStore } from '@/shared/stores/toastStore';
import { useFocusTrap } from '@/shared/hooks/useFocusTrap';
import { buildTeamUpdateData } from '@/features/settings/utils/teamUpdatePayload';
import { TeamForm } from './TeamForm';
import './TeamsPage.css';

interface Props {
  team?: Team | null;
  onClose: () => void;
  onCreated?: (team: Team) => void;
  onUpdated?: (team: Team) => void;
}

export function TeamDetailModal({ team = null, onClose, onCreated, onUpdated }: Props) {
  const { t } = useTranslation();
  const isEdit = !!team;
  const createTeam = useCreateTeam();
  const updateTeam = useUpdateTeam();
  const { addToast } = useToastStore();
  const modalRef = useRef<HTMLDivElement>(null);

  const [name, setName] = useState(team?.name ?? '');
  const [description, setDescription] = useState(team?.description ?? '');
  const [icon, setIcon] = useState(team?.icon || '👥');
  const [color, setColor] = useState(team?.color || '#6366f1');
  const [slackWebhookUrl, setSlackWebhookUrl] = useState(team?.slackWebhookUrl ?? '');
  const [prefix, setPrefix] = useState(team?.prefix ?? '');
  const [error, setError] = useState('');

  useEffect(() => {
    const rootElement = document.getElementById('root');
    if (rootElement) {
      rootElement.setAttribute('inert', '');
    }
    return () => {
      if (rootElement) {
        rootElement.removeAttribute('inert');
      }
    };
  }, []);

  useFocusTrap(modalRef, onClose);

  const isPending = createTeam.isPending || updateTeam.isPending;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');

    if (!name.trim()) {
      setError(t('team.modal.nameRequired'));
      return;
    }

    if (isEdit && !prefix.trim()) {
      setError(t('team.modal.prefixRequired'));
      return;
    }

    const trimmedPrefix = prefix.trim().toUpperCase();
    if (trimmedPrefix && !/^[A-Z0-9]{1,20}$/.test(trimmedPrefix)) {
      setError(t('team.modal.prefixInvalid'));
      return;
    }

    try {
      if (isEdit && team) {
        const data = buildTeamUpdateData({
          name,
          description,
          icon,
          color,
          slackWebhookUrl,
          prefix,
          isActive: team.isActive,
        });
        const updated = await updateTeam.mutateAsync({ id: team.id, data });
        addToast({ message: t('team.modal.updated'), type: 'success' });
        onUpdated?.(updated);
        onClose();
        return;
      }

      const created = await createTeam.mutateAsync({
        name: name.trim(),
        description: description.trim(),
        icon,
        color,
        slackWebhookUrl: slackWebhookUrl.trim() || undefined,
        ...(trimmedPrefix ? { prefix: trimmedPrefix } : {}),
      });
      addToast({ message: t('team.modal.created'), type: 'success' });
      onCreated?.(created);
      onClose();
    } catch {
      setError(isEdit ? t('team.modal.updateFailed') : t('team.modal.createFailed'));
    }
  };

  const modal = (
    <div
      className="teams-modal-overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="team-modal-title"
      onClick={onClose}
    >
      <div
        className="teams-modal"
        ref={modalRef}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="teams-modal__header">
          <h2 id="team-modal-title" className="teams-modal__title">
            {isEdit ? t('team.modal.titleEdit') : t('team.modal.titleCreate')}
          </h2>
          <button
            type="button"
            className="teams-modal__close"
            onClick={onClose}
            aria-label={t('common.close')}
          >
            ×
          </button>
        </div>
        <div className="teams-modal__form">
          <TeamForm
            name={name}
            prefix={prefix}
            description={description}
            icon={icon}
            color={color}
            slackWebhookUrl={slackWebhookUrl}
            isActive={team?.isActive ?? true}
            error={error}
            isPending={isPending}
            isEdit={isEdit}
            onNameChange={setName}
            onPrefixChange={setPrefix}
            onDescriptionChange={setDescription}
            onIconChange={setIcon}
            onColorChange={setColor}
            onWebhookChange={setSlackWebhookUrl}
            onActiveChange={() => {}}
            onSubmit={handleSubmit}
            onCancel={onClose}
            showActiveToggle={false}
            submitLabel={
              isPending
                ? t('team.modal.saving')
                : isEdit
                  ? t('team.modal.submitUpdate')
                  : t('team.modal.submitCreate')
            }
          />
        </div>
      </div>
    </div>
  );

  return createPortal(modal, document.body);
}
