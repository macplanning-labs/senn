import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useToastStore } from '../../../shared/stores/toastStore';
import { buildTicketShareUrl } from '../utils/ticketNavigation';
import type { TicketDetailView } from '../types/ticketDetailView';

interface TicketDetailSubBarProps {
  ticket: TicketDetailView;
}

export function TicketDetailSubBar({ ticket }: TicketDetailSubBarProps) {
  const { t } = useTranslation();
  const { addToast } = useToastStore();
  const [codingTool, setCodingTool] = useState<'Cursor' | 'Claude Code'>(
    () => (localStorage.getItem('senn.codingTool') as 'Cursor' | 'Claude Code') || 'Cursor',
  );

  const shareCtx = {
    projectKey: ticket.projectPrefix,
    teamSlug: ticket.team?.slug,
    ticketProjectPrefix: ticket.projectPrefix,
    ticketTeamSlug: ticket.team?.slug,
  };

  const handleCopyUrl = () => {
    const url = buildTicketShareUrl(window.location.origin, ticket.ticketKey, shareCtx);
    void navigator.clipboard.writeText(url);
    addToast({ message: t('ticketDetail.urlCopied'), type: 'success' });
  };

  const handleCopyId = () => {
    void navigator.clipboard.writeText(ticket.ticketKey);
    addToast({ message: t('ticketDetail.idCopied'), type: 'success' });
  };

  const handleCopyBranch = () => {
    const branch = `feat/${ticket.ticketKey.toLowerCase()}`;
    void navigator.clipboard.writeText(branch);
    addToast({ message: t('ticketDetail.branchCopied'), type: 'success' });
  };

  const handleCopyPrompt = () => {
    const prompt = [
      `Tool: ${codingTool}`,
      `Ticket: ${ticket.ticketKey}`,
      `Title: ${ticket.title}`,
      ticket.description ? `Description:\n${ticket.description}` : '',
    ]
      .filter(Boolean)
      .join('\n');
    void navigator.clipboard.writeText(prompt);
    addToast({ message: t('ticketDetail.promptCopied'), type: 'success' });
  };

  const handleToolChange = (tool: 'Cursor' | 'Claude Code') => {
    setCodingTool(tool);
    localStorage.setItem('senn.codingTool', tool);
  };

  return (
    <div className="ticket-detail-subbar">
      <button type="button" className="ticket-detail-subbar__button" onClick={handleCopyUrl} title={t('ticketDetail.copyUrl')}>
        {t('ticketDetail.url')}
      </button>
      <button type="button" className="ticket-detail-subbar__button" onClick={handleCopyId} title={t('ticketDetail.copyId')}>
        {t('ticketDetail.id')}
      </button>
      <button type="button" className="ticket-detail-subbar__button" onClick={handleCopyBranch} title={t('ticketDetail.branch')}>
        {t('ticketDetail.branch')}
      </button>
      <div className="ticket-detail-subbar__prompt-group">
        <button type="button" className="ticket-detail-subbar__button" onClick={handleCopyPrompt} title={t('ticketDetail.prompt')}>
          {t('ticketDetail.prompt')}
        </button>
        <select
          className="ticket-detail-subbar__tool-select"
          value={codingTool}
          onChange={(e) => handleToolChange(e.target.value as 'Cursor' | 'Claude Code')}
          aria-label={t('ticketDetail.codingTool')}
        >
          <option value="Cursor">Cursor</option>
          <option value="Claude Code">Claude Code</option>
        </select>
      </div>
    </div>
  );
}
