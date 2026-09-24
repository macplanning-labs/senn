/**
 * ProjectCreateModal.tsx — プロジェクト新規作成モーダル
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCreateProject } from '../hooks/useProjects';
import { useTeam } from '@/shared/hooks/useTeam';
import './ProjectCreateModal.css';
import { activeTeams as filterActiveTeams } from '@/features/teams/utils/archivedTeams';

interface Props {
  onClose: () => void;
  defaultTeamIds?: number[];
  onCreated?: (created: { prefix: string }) => void;
}

export function ProjectCreateModal({ onClose, defaultTeamIds, onCreated }: Props) {
  const { t } = useTranslation();
  const createProject = useCreateProject();
  const { teamList } = useTeam();
  const activeTeamList = filterActiveTeams(teamList);

  const [name, setName] = useState('');
  const [prefix, setPrefix] = useState('');
  const [description, setDescription] = useState('');
  const [priority, setPriority] = useState('medium');
  const [selectedTeamIds, setSelectedTeamIds] = useState<number[]>(defaultTeamIds ?? []);
  const [error, setError] = useState('');

  const toggleTeam = (teamId: number) => {
    setSelectedTeamIds((prev) =>
      prev.includes(teamId) ? prev.filter((id) => id !== teamId) : [...prev, teamId]
    );
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');

    if (!name.trim() || !prefix.trim()) {
      setError(t('sidebar.createValidation'));
      return;
    }
    if (selectedTeamIds.length === 0) {
      setError('参加チームを1つ以上選択してください');
      return;
    }

    try {
      await createProject.mutateAsync({
        name: name.trim(),
        prefix: prefix.trim().toUpperCase(),
        description: description.trim(),
        priority,
        teamIds: selectedTeamIds,
      });
      onCreated?.({ prefix: prefix.trim().toUpperCase() });
      onClose();
    } catch {
      setError(t('sidebar.createError'));
    }
  };

  return (
    <div className="project-create-modal-overlay" onClick={onClose}>
      <div className="project-create-modal" onClick={(e) => e.stopPropagation()}>
        <div className="project-create-modal__header">
          <h2 className="project-create-modal__title">{t('sidebar.newProject')}</h2>
          <button className="project-create-modal__close" onClick={onClose}>✕</button>
        </div>

        <form className="project-create-modal__form" onSubmit={(e) => void handleSubmit(e)}>
          <div className="project-create-modal__field">
            <label className="project-create-modal__label">{t('sidebar.projectName')} *</label>
            <input
              className="project-create-modal__input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
              data-testid="project-name-input"
            />
          </div>

          <div className="project-create-modal__field">
            <label className="project-create-modal__label">Prefix *</label>
            <input
              className="project-create-modal__input"
              value={prefix}
              onChange={(e) => setPrefix(e.target.value.toUpperCase())}
              placeholder="例: PRJ"
              data-testid="project-prefix-input"
            />
          </div>

          <div className="project-create-modal__field">
            <label className="project-create-modal__label">説明</label>
            <textarea
              className="project-create-modal__textarea"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
            />
          </div>

          <div className="project-create-modal__field">
            <label className="project-create-modal__label">{t('ticket.priority')}</label>
            <select
              className="project-create-modal__input"
              value={priority}
              onChange={(e) => setPriority(e.target.value)}
              data-testid="project-priority-select"
            >
              <option value="urgent">{t('ticket.priority_label.urgent')}</option>
              <option value="high">{t('ticket.priority_label.high')}</option>
              <option value="medium">{t('ticket.priority_label.medium')}</option>
              <option value="low">{t('ticket.priority_label.low')}</option>
            </select>
          </div>

          <div className="project-create-modal__field">
            <label className="project-create-modal__label">{t('sidebar.yourTeams')} *</label>
            <div className="project-create-modal__team-list">
              {activeTeamList.map((team) => (
                <label key={team.id} className="project-create-modal__team-checkbox">
                  <input
                    type="checkbox"
                    checked={selectedTeamIds.includes(team.id)}
                    onChange={() => toggleTeam(team.id)}
                  />
                  <span>{team.icon} {team.name}</span>
                </label>
              ))}
            </div>
          </div>

          {error && <div className="project-create-modal__error">{error}</div>}

          <div className="project-create-modal__footer">
            <button type="button" className="project-create-modal__btn project-create-modal__btn--cancel" onClick={onClose}>
              キャンセル
            </button>
            <button
              type="submit"
              className="project-create-modal__btn project-create-modal__btn--submit"
              disabled={createProject.isPending}
              data-testid="project-create-submit-btn"
            >
              {createProject.isPending ? '作成中...' : '作成'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
