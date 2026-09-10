/**
 * TeamsPage.tsx — チーム一覧ページ
 *
 * マスタメンテナンス方式（モーダル編集）で実装。
 * チーム一覧 + 作成/編集モーダル + メンバー管理セクション。
 */

import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useTeams, useDeleteTeam } from '../hooks/useTeams';
import { TeamDetailModal } from './TeamDetailModal';
import { TeamMembersSection } from './TeamMembersSection';
import { TeamGuestsSection } from './TeamGuestsSection';
import { TeamRulesSection } from './TeamRulesSection';
import type { Team } from '@/shared/api/types';
import { useToastStore } from '@/shared/stores/toastStore';
import { useUIStore } from '@/shared/stores/uiStore';
import './TeamsPage.css';
import { useTranslation } from 'react-i18next';

export function TeamsPage() {
  const { t } = useTranslation();
  const { data: teams, isLoading } = useTeams();
  const deleteTeam = useDeleteTeam();
  const { addToast } = useToastStore();
  const { openTicketFormModal } = useUIStore();

  // モーダル制御
  const [modalOpen, setModalOpen] = useState(false);
  const [editingTeam, setEditingTeam] = useState<Team | null>(null);
  const [selectedTeam, setSelectedTeam] = useState<Team | null>(null);

  // 確認ダイアログ
  const [deleteConfirm, setDeleteConfirm] = useState<number | null>(null);

  const handleCreate = () => {
    setEditingTeam(null);
    setModalOpen(true);
  };

  const handleEdit = (team: Team) => {
    setEditingTeam(team);
    setModalOpen(true);
  };

  const handleDelete = async (id: number) => {
    try {
      await deleteTeam.mutateAsync(id);
      addToast({ message: 'チームを削除しました', type: 'success' });
      setDeleteConfirm(null);
      if (selectedTeam?.id === id) setSelectedTeam(null);
    } catch {
      addToast({ message: 'チームの削除に失敗しました', type: 'error' });
    }
  };

  const handleModalClose = () => {
    setModalOpen(false);
    setEditingTeam(null);
  };

  if (isLoading) {
    return (
      <div className="teams-loading">
        <div className="teams-loading__spinner" />
      </div>
    );
  }

  return (
    <div className="teams-page">
      <div className="teams-page__header">
        <div>
          <h1 className="teams-page__title">Teams</h1>
          <p className="teams-page__subtitle">
            所属 Team が主な作業単位です。横断 Project への参加は任意の追加経路です。
          </p>
        </div>
        <button
          className="teams-page__create-btn"
          onClick={handleCreate}
          data-testid="create-team-btn"
        >
          <span className="teams-page__create-icon">+</span>
          新規チーム
        </button>
      </div>

      <div className="teams-page__content">
        {/* チーム一覧 */}
        <div className="teams-page__list">
          {teams && teams.length > 0 ? (
            teams.map((team) => (
              <div
                key={team.id}
                className={`teams-card ${selectedTeam?.id === team.id ? 'teams-card--selected' : ''}`}
                onClick={() => setSelectedTeam(team)}
                data-testid={`team-card-${team.id}`}
              >
                <div className="teams-card__header">
                  <span
                    className="teams-card__icon"
                    style={{ backgroundColor: team.color + '20', color: team.color }}
                  >
                    {team.icon}
                  </span>
                  <div className="teams-card__info">
                    <span className="teams-card__name">{team.name}</span>
                    <span className="teams-card__slug">@{team.slug}</span>
                  </div>
                  <div className="teams-card__actions">
                    <button
                      className="teams-card__action-btn"
                      onClick={(e) => { e.stopPropagation(); handleEdit(team); }}
                      title="編集"
                      data-testid={`edit-team-${team.id}`}
                    >
                      ✏️
                    </button>
                    <button
                      className="teams-card__action-btn teams-card__action-btn--danger"
                      onClick={(e) => { e.stopPropagation(); setDeleteConfirm(team.id); }}
                      title="削除"
                      data-testid={`delete-team-${team.id}`}
                    >
                      🗑️
                    </button>
                  </div>
                </div>
                {team.description && (
                  <p className="teams-card__desc">{team.description}</p>
                )}
                <div className="teams-card__stats">
                  <span className="teams-card__stat">
                    <span className="teams-card__stat-icon">👤</span>
                    {team.memberCount} メンバー
                  </span>
                  <span className="teams-card__stat">
                    <span className="teams-card__stat-icon">📁</span>
                    {team.projectCount} プロジェクト
                  </span>
                  <Link
                    to={`/t/${team.slug}/tickets`}
                    className="teams-card__tickets-link"
                    onClick={(e) => e.stopPropagation()}
                    data-testid={`team-tickets-${team.id}`}
                  >
                    チケット
                  </Link>
                </div>
                {!team.isActive && (
                  <span className="teams-card__inactive-badge">無効</span>
                )}
              </div>
            ))
          ) : (
            <div className="teams-page__empty">
              <span className="teams-page__empty-icon">👥</span>
              <p>{t('team.noTeams')}</p>
              <button className="teams-page__create-btn" onClick={handleCreate}>
                最初のチームを作成
              </button>
            </div>
          )}
        </div>

        {/* メンバー管理パネル */}
        {selectedTeam && (
          <div className="teams-page__detail">
            <div className="teams-page__detail-actions">
              <Link
                to={`/t/${selectedTeam.slug}/tickets`}
                className="teams-page__tickets-btn"
                data-testid="selected-team-tickets"
              >
                {selectedTeam.name} のチケット
              </Link>
              <button
                type="button"
                className="teams-page__create-ticket-btn"
                onClick={() => openTicketFormModal(null, selectedTeam.slug)}
                data-testid="selected-team-create-ticket"
              >
                + チケット作成
              </button>
            </div>
            <TeamMembersSection team={selectedTeam} />
            <TeamGuestsSection team={selectedTeam} />
            <TeamRulesSection teamId={selectedTeam.id} />
          </div>
        )}
      </div>

      {/* 作成/編集モーダル */}
      {modalOpen && (
        <TeamDetailModal
          team={editingTeam}
          onClose={handleModalClose}
        />
      )}

      {/* 削除確認ダイアログ */}
      {deleteConfirm !== null && (
        <div className="teams-dialog-overlay" onClick={() => setDeleteConfirm(null)}>
          <div className="teams-dialog" onClick={(e) => e.stopPropagation()}>
            <h3 className="teams-dialog__title">{t('team.deleteConfirm')}</h3>
            <p className="teams-dialog__message">
              この操作は取り消せません。チームのメンバーシップも削除されます。
            </p>
            <div className="teams-dialog__actions">
              <button
                className="teams-dialog__btn teams-dialog__btn--cancel"
                onClick={() => setDeleteConfirm(null)}
              >
                キャンセル
              </button>
              <button
                className="teams-dialog__btn teams-dialog__btn--danger"
                onClick={() => void handleDelete(deleteConfirm)}
                data-testid="confirm-delete-team"
              >
                削除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
