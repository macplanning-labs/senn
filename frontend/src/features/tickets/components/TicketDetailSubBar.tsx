import { useState, useRef } from 'react';
import { useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useToastStore } from '../../../shared/stores/toastStore';
import { buildTicketPathContextFromPathname, buildTicketShareUrl } from '../utils/ticketNavigation';
import { buildCopyPrompt } from '../utils/buildCopyPrompt';
import { generateAndCopyAiPrompt } from '../utils/generateAndCopyAiPrompt';
import { useAiPromptTemplates } from '../../../features/settings/hooks/useAiPromptTemplates';
import type { TicketDetailView } from '../types/ticketDetailView';

interface TicketDetailSubBarProps {
  ticket: TicketDetailView;
}

export function TicketDetailSubBar({ ticket }: TicketDetailSubBarProps) {
  const { t } = useTranslation();
  const { pathname } = useLocation();
  const { addToast } = useToastStore();
  const [codingTool, setCodingTool] = useState<'Cursor' | 'Claude Code'>(
    () => (localStorage.getItem('senn.codingTool') as 'Cursor' | 'Claude Code') || 'Cursor',
  );
  const isGeneratingRef = useRef(false);
  const { templates: userTemplates } = useAiPromptTemplates();

  const shareCtx = buildTicketPathContextFromPathname(pathname, ticket);

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

  const defaults = {
    common: t('settings.defaultAiPromptTemplate'),
    cursor: t('settings.defaultAiPromptTemplateCursor'),
    claude: t('settings.defaultAiPromptTemplateClaude'),
  };

  const handleCopyPrompt = async () => {
    // 生成済みキャッシュ（チケット内容のみ）を優先する
    const cached = ticket.aiPrompt?.trim();
    if (cached) {
      // 個人設定のおまじないと結合
      const finalPrompt = buildCopyPrompt(cached, codingTool, userTemplates, defaults);
      void navigator.clipboard.writeText(finalPrompt);
      addToast({ message: t('ticketDetail.aiPromptCopied', { ticketKey: ticket.ticketKey }), type: 'success' });
      return;
    }

    // プロンプトが未生成の場合、その場で生成
    if (!isGeneratingRef.current) addToast({ message: t('ticketDetail.aiPromptGenerating'), type: 'info' });
    const result = await generateAndCopyAiPrompt(
      ticket.ticketKey,
      isGeneratingRef,
      codingTool,
      userTemplates,
      defaults,
      t,
    );

    if (!result.success && result.toast.type === 'info') {
      // 生成失敗時は、簡易プロンプトをコピー
      const prompt = [
        `Tool: ${codingTool}`,
        `Ticket: ${ticket.ticketKey}`,
        `Title: ${ticket.title}`,
        ticket.description ? `Description:\n${ticket.description}` : '',
      ]
        .filter(Boolean)
        .join('\n');
      void navigator.clipboard.writeText(prompt);
    }

    addToast(result.toast);
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
