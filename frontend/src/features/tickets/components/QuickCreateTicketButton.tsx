/**
 * QuickCreateTicketButton.tsx — ワンクリック起票ボタン
 *
 * Wikiページ、Triage依頼などからチケットを素早く作成。
 * 起票元の情報を自動で description に埋め込む。
 */
import { useMemo, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { apiClient } from '@/shared/api/client';
import { ensureTicketLocal } from '@/shared/sync/repos/ticketRepo';
import { useProjectById } from '@/shared/sync/repos/projectRepo';
import { buildTicketDetailPath } from '../utils/ticketNavigation';
import { resolveQuickCreateTeam } from '../utils/quickCreateTeam';

interface Props {
  defaultTitle?: string;
  defaultDescription?: string;
  /** 起票先プロジェクト。チームWiki・全体Wikiのように無い場合は null */
  projectId?: number | null;
  /**
   * 起票先チーム。チケットは必ずチームに属する(API が teamId を要求する)。
   * 未指定ならプロジェクトの参加チームから決める(1つなら自動、複数なら選択)。
   */
  teamId?: number | null;
  /** teamId を明示した場合の、そのチームの slug(完了表示のリンク先に使う) */
  teamSlug?: string | null;
  /** 起票元がWikiの場合、起票後に自動リンク */
  wikiPageId?: number;
  onCreated?: (ticketId: number) => void;
  /** ボタンラベルカスタマイズ */
  label?: string;
}

interface CreatedTicket {
  id: number;
  ticketKey: string;
  title: string;
}

export function QuickCreateTicketButton({
  defaultTitle = '',
  defaultDescription = '',
  projectId = null,
  teamId = null,
  teamSlug = null,
  wikiPageId,
  onCreated,
  label,
}: Props) {
  const { t } = useTranslation();
  const [showForm, setShowForm] = useState(false);
  const [title, setTitle] = useState(defaultTitle);
  const [created, setCreated] = useState<CreatedTicket | null>(null);
  const [selectedTeamId, setSelectedTeamId] = useState<number | null>(null);
  const queryClient = useQueryClient();
  const project = useProjectById(projectId);

  // 起票先チーム(指定が無ければプロジェクトの参加チームから決める)
  const { teamId: resolvedTeamId, needsChoice: needsTeamChoice, choices: teamChoices } = useMemo(
    () => resolveQuickCreateTeam(teamId, project?.teams ?? [], selectedTeamId),
    [teamId, project, selectedTeamId],
  );
  const createdTeamSlug = teamSlug ?? project?.teams.find((tm) => tm.id === resolvedTeamId)?.slug ?? null;

  const mutation = useMutation({
    mutationFn: async () => {
      // 1. チケット作成
      const res = await apiClient.post<CreatedTicket>('/tickets/', {
        title: title || defaultTitle || 'New ticket',
        description: defaultDescription,
        project: projectId,
        team_id: resolvedTeamId,
        ticket_type: 'task',
        status: 'open',
        priority: 'medium',
      });
      const ticket = res.data;

      // 2. Wikiリンク自動作成
      if (wikiPageId) {
        await apiClient.post(`/wiki/${wikiPageId}/link-ticket/`, {
          ticket_id: ticket.id,
        });
      }

      // 3. 端末内 DB へ書き込む
      await ensureTicketLocal(ticket.ticketKey);

      return ticket;
    },
    onSuccess: (ticket) => {
      setCreated(ticket);
      setShowForm(false);
      void queryClient.invalidateQueries({ queryKey: ['wiki'] });
      onCreated?.(ticket.id);
    },
  });

  if (created) {
    return (
      <div style={{
        display: 'inline-flex', alignItems: 'center', gap: '0.5rem',
        padding: '0.375rem 0.75rem', borderRadius: '6px',
        background: 'rgba(45, 164, 78, 0.15)', color: 'var(--color-success, #2da44e)',
        fontSize: '0.8125rem',
      }}>
        ✅ <Link
          to={buildTicketDetailPath(project?.prefix ?? null, created.ticketKey, undefined, createdTeamSlug)}
          style={{ color: 'inherit', textDecoration: 'underline' }}
        >
          {created.ticketKey}
        </Link> {t('ticket.filedSuffix')}
      </div>
    );
  }

  if (showForm) {
    const canSubmit = !!title.trim() && resolvedTeamId != null && !mutation.isPending;
    const errorDetail = (mutation.error as { response?: { data?: { detail?: string } } } | null)
      ?.response?.data?.detail;
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: '0.375rem' }}>
      <div style={{
        display: 'flex', gap: '0.5rem', alignItems: 'center',
      }}>
        {needsTeamChoice ? (
          <select
            aria-label={t('ticket.quickCreateTeamLabel')}
            value={selectedTeamId ?? ''}
            onChange={(e) => setSelectedTeamId(e.target.value ? Number(e.target.value) : null)}
            style={{
              padding: '0.375rem 0.5rem',
              borderRadius: '6px', border: '1px solid var(--color-border-default, #e0e0e0)',
              fontSize: '0.8125rem',
              background: 'var(--color-bg-primary, #fff)',
              color: 'var(--color-text-primary, #1a1a1a)',
            }}
          >
            <option value="">{t('ticket.quickCreateSelectTeam')}</option>
            {teamChoices.map((tm) => (
              <option key={tm.id} value={tm.id}>{tm.name}</option>
            ))}
          </select>
        ) : null}
        <input
          type="text"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={t('ticket.titleInputPlaceholder')}
          style={{
            flex: 1, padding: '0.375rem 0.5rem',
            borderRadius: '6px', border: '1px solid var(--color-border-default, #e0e0e0)',
            fontSize: '0.8125rem',
            background: 'var(--color-bg-primary, #fff)',
            color: 'var(--color-text-primary, #1a1a1a)',
          }}
          autoFocus
          onKeyDown={(e) => {
            if (e.key === 'Enter' && canSubmit) mutation.mutate();
            if (e.key === 'Escape') setShowForm(false);
          }}
        />
        <button
          onClick={() => mutation.mutate()}
          disabled={!canSubmit}
          style={{
            padding: '0.375rem 0.625rem', borderRadius: '6px',
            border: 'none', fontSize: '0.8125rem', cursor: 'pointer',
            background: 'var(--color-success, #2da44e)', color: '#fff',
            opacity: canSubmit ? 1 : 0.5,
          }}
        >
          {mutation.isPending ? t('common.creating') : t('common.create')}
        </button>
        <button
          onClick={() => setShowForm(false)}
          style={{
            padding: '0.375rem 0.5rem', borderRadius: '6px',
            border: 'none', fontSize: '0.8125rem', cursor: 'pointer',
            background: 'var(--color-bg-tertiary, #f5f5f5)',
            color: 'var(--color-text-primary)',
          }}
        >
          ✕
        </button>
      </div>
      {resolvedTeamId == null && !needsTeamChoice ? (
        <div role="alert" style={{ fontSize: '0.75rem', color: 'var(--color-danger, #cf222e)' }}>
          {t('ticket.quickCreateNoTeam')}
        </div>
      ) : null}
      {mutation.isError ? (
        <div role="alert" style={{ fontSize: '0.75rem', color: 'var(--color-danger, #cf222e)' }}>
          {t('ticket.quickCreateFailed')}{errorDetail ? `: ${errorDetail}` : ''}
        </div>
      ) : null}
      </div>
    );
  }

  return (
    <button
      onClick={() => {
        setTitle(defaultTitle);
        mutation.reset();
        setShowForm(true);
      }}
      style={{
        padding: '0.375rem 0.75rem', borderRadius: '6px',
        border: '1px solid var(--color-border-default, #e0e0e0)',
        background: 'var(--color-bg-tertiary, #f5f5f5)',
        color: 'var(--color-text-primary)',
        fontSize: '0.8125rem', cursor: 'pointer',
        transition: 'all 0.15s',
      }}
    >
      {label ?? `⚡ ${t('ticket.fileTicketButton')}`}
    </button>
  );
}
