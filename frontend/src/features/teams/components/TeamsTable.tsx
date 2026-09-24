/**
 * TeamsTable — プロジェクト一覧に倣ったチーム行テーブル
 *
 * 行クリック → チーム詳細（作業）。操作列 → 編集・アーカイブ／復元（管理）。
 */

import { Link, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import type { Team } from '@/shared/api/types';
import { ArchiveTeamButton } from './ArchiveTeamButton';
import { RestoreTeamButton } from './RestoreTeamButton';
import './TeamsTable.css';

interface TeamsTableProps {
  teams: Team[];
  archived?: boolean;
  onEdit?: (team: Team) => void;
}

export function TeamsTable({ teams, archived = false, onEdit }: TeamsTableProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();

  if (teams.length === 0) return null;

  return (
    <table className="teams-table" data-testid={archived ? 'teams-table-archived' : 'teams-table'}>
      <thead>
        <tr>
          <th className="teams-table__th teams-table__th--name">{t('team.list.name')}</th>
          <th className="teams-table__th teams-table__th--desc">{t('team.list.description')}</th>
          <th className="teams-table__th">{t('team.list.members')}</th>
          <th className="teams-table__th">{t('team.list.projects')}</th>
          <th className="teams-table__th">{t('team.list.actions')}</th>
        </tr>
      </thead>
      <tbody>
        {teams.map((team) => (
          <tr
            key={team.id}
            className="teams-table__row"
            data-testid={`team-row-${team.id}`}
            onClick={() => navigate(`/team/${team.slug}`)}
          >
            <td className="teams-table__td teams-table__td--name">
              <Link
                to={`/team/${team.slug}`}
                className="teams-table__link"
                onClick={(e) => e.stopPropagation()}
              >
                <span
                  className="teams-table__icon"
                  style={{ backgroundColor: `${team.color}33`, color: team.color }}
                  aria-hidden="true"
                >
                  {team.icon || team.name[0]?.toUpperCase() || 'T'}
                </span>
                <span className="teams-table__name-block">
                  <span className="teams-table__name">{team.name}</span>
                  <span className="teams-table__slug">@{team.slug}</span>
                </span>
              </Link>
              {archived && (
                <span className="teams-table__badge">{t('teamArchive.badge')}</span>
              )}
            </td>
            <td className="teams-table__td teams-table__td--desc">
              {team.description?.trim() ? team.description : '—'}
            </td>
            <td className="teams-table__td">{team.memberCount}</td>
            <td className="teams-table__td">{team.projectCount}</td>
            <td
              className="teams-table__td teams-table__td--actions"
              onClick={(e) => e.stopPropagation()}
            >
              {archived ? (
                <RestoreTeamButton team={team} />
              ) : (
                <>
                  {team.viewerCanManage && onEdit && (
                    <button
                      type="button"
                      className="teams-table__action-btn"
                      onClick={() => onEdit(team)}
                      data-testid={`team-edit-btn-${team.id}`}
                    >
                      {t('common.edit')}
                    </button>
                  )}
                  <ArchiveTeamButton team={team} />
                </>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
