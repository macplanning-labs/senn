import { useEffect, useRef, useState } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useOptimisticMutation } from '../../../shared/hooks/useOptimisticMutation';
import {
  buildTicketListPath,
  buildTicketShareUrl,
  detectTicketDetailOrigin,
} from '../utils/ticketNavigation';
import { apiClient } from '../../../shared/api/client';
import { useToastStore } from '../../../shared/stores/toastStore';
import type { TicketDetailView } from '../types/ticketDetailView';
import { IconMoreHorizontal } from '../../../shared/components/ui/icons';

interface TicketDetailTopBarProps {
  ticket: TicketDetailView;
  ticketId: string;
}

export function TicketDetailTopBar({ ticket, ticketId }: TicketDetailTopBarProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const queryClient = useQueryClient();
  const { addToast } = useToastStore();
  const [menuOpen, setMenuOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const onDoc = (e: MouseEvent) => {
      if (!menuRef.current?.contains(e.target as Node)) {
        setMenuOpen(false);
        setConfirmDelete(false);
      }
    };
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, [menuOpen]);

  const shareCtx = {
    projectKey: ticket.projectPrefix,
    teamSlug: ticket.team?.slug,
    ticketProjectPrefix: ticket.projectPrefix,
    ticketTeamSlug: ticket.team?.slug,
  };

  const watchMutation = useOptimisticMutation<void, { watch: boolean }>({
    mutationFn: async ({ watch }) => {
      if (watch) {
        await apiClient.post(`/tickets/${ticketId}/watch/`);
      } else {
        await apiClient.delete(`/tickets/${ticketId}/watch/`);
      }
    },
    queryKey: ['ticket', ticketId],
    updater: (currentData, { watch }) => {
      const data = currentData as TicketDetailView | undefined;
      if (!data) return currentData;
      return { ...data, isWatching: watch };
    },
    errorMessage: t('ticketDetail.watchError'),
  });

  const deleteMutation = useMutation({
    mutationFn: async () => {
      await apiClient.delete(`/tickets/${ticketId}/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
      navigate(buildTicketListPath(ticket.projectPrefix, undefined, ticket.team?.slug));
    },
    onError: () => {
      addToast({ message: t('ticketDetail.deleteError'), type: 'error' });
      setConfirmDelete(false);
    },
  });

  const teamSlug = ticket.team?.slug ?? null;
  const projectKey = ticket.projectPrefix ?? null;
  const scopeLabel = ticket.team?.name || projectKey || t('nav.backTo.tickets');
  const ticketsLabel = t('ticketDetail.tickets');
  const listPath = buildTicketListPath(projectKey, undefined, teamSlug);
  // スコープ根: チームならチーム画面、プロジェクトならプロジェクト概要
  const scopePath = teamSlug
    ? `/team/${teamSlug}`
    : projectKey
      ? `/project/${projectKey}`
      : listPath;
  const isWatching = ticket.isWatching;

  // パンくずは、詳細を開いた入口（自分のチケット・ボード・サイクル・一覧）に合わせる
  const origin = detectTicketDetailOrigin(pathname);
  const originScopeLabel = (base: string) =>
    base.startsWith('/team/') ? ticket.team?.name || base.replace(/^\/team\//, '') : base.replace(/^\/project\//, '');
  const crumbs: { label: string; to: string; testId: string }[] =
    origin.kind === 'myIssues'
      ? [{ label: t('nav.myIssues'), to: '/my-issues', testId: 'crumb-my-issues' }]
      : origin.kind === 'board'
        ? [
            { label: originScopeLabel(origin.base), to: origin.base, testId: 'crumb-scope' },
            { label: t('nav.board'), to: `${origin.base}/board`, testId: 'crumb-board' },
          ]
        : origin.kind === 'cycle'
          ? [
              { label: originScopeLabel(origin.base), to: origin.base, testId: 'crumb-scope' },
              { label: t('nav.cycles'), to: `${origin.base}/cycles/${origin.cycleId}`, testId: 'crumb-cycle' },
            ]
          : [
              { label: scopeLabel, to: scopePath, testId: 'crumb-scope' },
              { label: ticketsLabel, to: listPath, testId: 'crumb-tickets' },
            ];

  const copyUrl = () => {
    const url = buildTicketShareUrl(window.location.origin, ticket.ticketKey, shareCtx);
    void navigator.clipboard.writeText(url);
    addToast({ message: t('ticketDetail.urlCopied'), type: 'success' });
    setMenuOpen(false);
  };

  const copyId = () => {
    void navigator.clipboard.writeText(ticket.ticketKey);
    addToast({ message: t('ticketDetail.idCopied'), type: 'success' });
    setMenuOpen(false);
  };

  return (
    <div className="ticket-detail-topbar">
      <div className="ticket-detail-topbar__breadcrumb-wrapper">
        <nav className="ticket-detail-topbar__breadcrumb" aria-label="Breadcrumb">
          {crumbs.map((crumb) => (
            <span key={crumb.testId} className="ticket-detail-topbar__breadcrumb-item">
              <Link
                to={crumb.to}
                className="ticket-detail-topbar__breadcrumb-link"
                data-testid={crumb.testId}
              >
                {crumb.label}
              </Link>
              <span className="ticket-detail-topbar__breadcrumb-sep" aria-hidden="true">
                {' > '}
              </span>
            </span>
          ))}
          <span className="ticket-detail-topbar__breadcrumb-current" aria-current="page">
            {ticket.ticketKey}
          </span>
        </nav>
      </div>
      <div className="ticket-detail-topbar__actions">
        <button
          type="button"
          className="ticket-detail-topbar__watch"
          onClick={() => watchMutation.mutate({ watch: !isWatching })}
          title={isWatching ? t('ticketDetail.unwatchTitle') : t('ticketDetail.watchTitle')}
          aria-label={isWatching ? t('ticketDetail.unwatchTitle') : t('ticketDetail.watchTitle')}
          disabled={watchMutation.isPending}
          data-testid="ticket-detail-watch"
        >
          {isWatching ? '🔔' : '🔕'}
        </button>
        <div className="ticket-detail-topbar__menu-wrap" ref={menuRef}>
          <button
            type="button"
            className="ticket-detail-topbar__menu"
            title={t('ticketDetail.menu')}
            aria-expanded={menuOpen}
            aria-haspopup="menu"
            onClick={() => {
              setMenuOpen((v) => !v);
              setConfirmDelete(false);
            }}
            aria-label={t('ticketDetail.menu')}
            data-testid="ticket-detail-more"
          >
            <IconMoreHorizontal size={16} />
          </button>
          {menuOpen && (
            <div className="ticket-detail-topbar__dropdown" role="menu">
              <button type="button" role="menuitem" onClick={copyUrl}>
                {t('ticketDetail.copyUrl')}
              </button>
              <button type="button" role="menuitem" onClick={copyId}>
                {t('ticketDetail.copyId')}
              </button>
              {!confirmDelete ? (
                <button
                  type="button"
                  role="menuitem"
                  className="ticket-detail-topbar__dropdown-danger"
                  onClick={() => setConfirmDelete(true)}
                >
                  {t('ticketDetail.delete')}
                </button>
              ) : (
                <button
                  type="button"
                  role="menuitem"
                  className="ticket-detail-topbar__dropdown-danger"
                  disabled={deleteMutation.isPending}
                  onClick={() => deleteMutation.mutate()}
                  data-testid="ticket-detail-delete-confirm"
                >
                  {t('ticketDetail.deleteConfirm')}
                </button>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
