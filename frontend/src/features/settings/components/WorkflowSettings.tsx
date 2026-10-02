/**
 * WorkflowSettings.tsx — ワークフローステータス管理UI
 *
 * プロジェクト設定ページ内のタブ。
 * ステータスの追加・編集・削除・並び替えが可能。
 */
import { useState } from 'react';
import { useProject } from '@/shared/hooks/useProject';
import {
  useWorkflowStatuses,
  useCreateWorkflowStatus,
  useUpdateWorkflowStatus,
  useDeleteWorkflowStatus,
} from '../hooks/useWorkflowStatuses';
import type { WorkflowStatus, StatusCategory } from '@/shared/api/types';
import './WorkflowSettings.css';
import { useTranslation } from 'react-i18next';

function getCategoryOptions(t: (key: string) => string): { value: StatusCategory; label: string; color: string }[] {
  return [
    { value: 'backlog', label: t('settings.statusPresetBacklog'), color: '#666666' },
    { value: 'unstarted', label: t('settings.statusPresetUnstarted'), color: '#a0a0a0' },
    { value: 'started', label: t('settings.statusPresetStarted'), color: '#f5a623' },
    { value: 'completed', label: t('settings.statusPresetCompleted'), color: '#50e3c2' },
    { value: 'cancelled', label: t('settings.statusPresetCancelled'), color: '#ff4d4f' },
  ];
}

export function WorkflowSettings({ teamId }: { teamId?: number } = {}) {
  const { t } = useTranslation();
  const { currentProject } = useProject();
  const projectId = teamId ? undefined : currentProject?.id;
  const { data: statuses = [], isLoading } = useWorkflowStatuses(projectId, teamId);
  const createMutation = useCreateWorkflowStatus();
  const updateMutation = useUpdateWorkflowStatus();
  const deleteMutation = useDeleteWorkflowStatus();

  const [newName, setNewName] = useState('');
  const [newSlug, setNewSlug] = useState('');
  const [newCategory, setNewCategory] = useState<StatusCategory>('unstarted');
  const [newColor, setNewColor] = useState('#a0a0a0');
  const [editingId, setEditingId] = useState<number | null>(null);

  if (!teamId && !currentProject) {
    return <p className="workflow-settings__empty">{t('settings.selectProject')}</p>;
  }

  const handleCreate = () => {
    if (!newName.trim() || !newSlug.trim()) return;
    createMutation.mutate({
      project: teamId ? null : currentProject?.id,
      teamId: teamId ?? null,
      name: newName.trim(),
      slug: newSlug.trim().toLowerCase().replace(/\s+/g, '_'),
      category: newCategory,
      color: newColor,
      position: statuses.length,
      isDefault: false,
    }, {
      onSuccess: () => {
        setNewName('');
        setNewSlug('');
        setNewCategory('unstarted');
        setNewColor('#a0a0a0');
      },
    });
  };

  const handleUpdate = (status: WorkflowStatus, field: string, value: string | boolean) => {
    updateMutation.mutate({
      id: status.id,
      [field]: value,
    });
  };

  const handleDelete = (status: WorkflowStatus) => {
    if (status.isDefault) return;
    deleteMutation.mutate({ id: status.id, projectId: currentProject?.id, teamId });
  };

  if (isLoading) return <p>Loading...</p>;

  return (
    <div className="workflow-settings">
      <div className="workflow-settings__header">
        <h3 className="workflow-settings__title">{t('settings.workflowStatusTitle')}</h3>
        <p className="workflow-settings__desc">
          {teamId
            ? t('settings.workflowStatusHintTeam')
            : t('settings.workflowStatusHintProject')}
        </p>
      </div>

      {/* ステータス一覧 */}
      <div className="workflow-settings__list">
        {statuses.map((status) => (
          <div key={status.id} className="workflow-status-row">
            <span
              className="workflow-status-row__color"
              style={{ backgroundColor: status.color }}
            />
            {editingId === status.id ? (
              <input
                className="workflow-status-row__input"
                value={status.name}
                onChange={(e) => handleUpdate(status, 'name', e.target.value)}
                onBlur={() => setEditingId(null)}
                autoFocus
              />
            ) : (
              <span
                className="workflow-status-row__name"
                onClick={() => setEditingId(status.id)}
              >
                {status.name}
              </span>
            )}
            <span className="workflow-status-row__slug">{status.slug}</span>
            <select
              className="workflow-status-row__category"
              value={status.category}
              onChange={(e) => handleUpdate(status, 'category', e.target.value)}
            >
              {getCategoryOptions(t).map(opt => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
            <input
              type="color"
              className="workflow-status-row__color-picker"
              value={status.color}
              onChange={(e) => handleUpdate(status, 'color', e.target.value)}
            />
            {status.isDefault && (
              <span className="workflow-status-row__badge">Default</span>
            )}
            {!status.isDefault && (
              <button
                className="workflow-status-row__delete"
                onClick={() => handleDelete(status)}
                title={t('common.delete')}
              >
                ×
              </button>
            )}
          </div>
        ))}
      </div>

      {/* 新規追加フォーム */}
      <div className="workflow-settings__add">
        <input
          className="workflow-settings__add-input"
          placeholder={t('settings.statusNamePlaceholder')}
          value={newName}
          onChange={(e) => {
            setNewName(e.target.value);
            setNewSlug(e.target.value.toLowerCase().replace(/\s+/g, '_'));
          }}
        />
        <input
          className="workflow-settings__add-input workflow-settings__add-input--slug"
          placeholder="slug"
          value={newSlug}
          onChange={(e) => setNewSlug(e.target.value)}
        />
        <select
          className="workflow-settings__add-select"
          value={newCategory}
          onChange={(e) => setNewCategory(e.target.value as StatusCategory)}
        >
          {getCategoryOptions(t).map(opt => (
            <option key={opt.value} value={opt.value}>{opt.label}</option>
          ))}
        </select>
        <input
          type="color"
          className="workflow-settings__add-color"
          value={newColor}
          onChange={(e) => setNewColor(e.target.value)}
        />
        <button
          className="workflow-settings__add-btn"
          onClick={handleCreate}
          disabled={!newName.trim() || !newSlug.trim() || createMutation.isPending}
        >
          {t('settings.addStatus')}
        </button>
      </div>
    </div>
  );
}
