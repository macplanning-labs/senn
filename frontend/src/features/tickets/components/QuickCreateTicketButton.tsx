/**
 * QuickCreateTicketButton.tsx — ワンクリック起票ボタン
 *
 * Wikiページ、Triage依頼などからチケットを素早く作成。
 * 起票元の情報を自動で description に埋め込む。
 */
import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';

interface Props {
  defaultTitle?: string;
  defaultDescription?: string;
  projectId: number;
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
  projectId,
  wikiPageId,
  onCreated,
  label = '⚡ チケット起票',
}: Props) {
  const [showForm, setShowForm] = useState(false);
  const [title, setTitle] = useState(defaultTitle);
  const [created, setCreated] = useState<CreatedTicket | null>(null);
  const queryClient = useQueryClient();

  const mutation = useMutation({
    mutationFn: async () => {
      // 1. チケット作成
      const res = await apiClient.post<CreatedTicket>('/tickets/', {
        title: title || defaultTitle || 'New ticket',
        description: defaultDescription,
        project: projectId,
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

      return ticket;
    },
    onSuccess: (ticket) => {
      setCreated(ticket);
      setShowForm(false);
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
      void queryClient.invalidateQueries({ queryKey: ['wiki'] });
      onCreated?.(ticket.id);
    },
  });

  if (created) {
    return (
      <div style={{
        display: 'inline-flex', alignItems: 'center', gap: '0.5rem',
        padding: '0.375rem 0.75rem', borderRadius: '6px',
        background: 'rgba(45, 164, 78, 0.15)', color: 'var(--success, #2da44e)',
        fontSize: '0.8125rem',
      }}>
        ✅ <a
          href={`/p/${created.ticketKey?.split('-')[0] ?? 'XX'}/tickets/${created.id}`}
          style={{ color: 'inherit', textDecoration: 'underline' }}
        >
          {created.ticketKey}
        </a> を起票しました
      </div>
    );
  }

  if (showForm) {
    return (
      <div style={{
        display: 'flex', gap: '0.5rem', alignItems: 'center',
      }}>
        <input
          type="text"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="チケットタイトル..."
          style={{
            flex: 1, padding: '0.375rem 0.5rem',
            borderRadius: '6px', border: '1px solid var(--border-subtle, #e0e0e0)',
            fontSize: '0.8125rem',
            background: 'var(--surface-primary, #fff)',
            color: 'var(--color-text-primary, #1a1a1a)',
          }}
          autoFocus
          onKeyDown={(e) => {
            if (e.key === 'Enter' && title.trim()) mutation.mutate();
            if (e.key === 'Escape') setShowForm(false);
          }}
        />
        <button
          onClick={() => mutation.mutate()}
          disabled={!title.trim() || mutation.isPending}
          style={{
            padding: '0.375rem 0.625rem', borderRadius: '6px',
            border: 'none', fontSize: '0.8125rem', cursor: 'pointer',
            background: 'var(--success, #2da44e)', color: '#fff',
            opacity: !title.trim() || mutation.isPending ? 0.5 : 1,
          }}
        >
          {mutation.isPending ? '作成中...' : '作成'}
        </button>
        <button
          onClick={() => setShowForm(false)}
          style={{
            padding: '0.375rem 0.5rem', borderRadius: '6px',
            border: 'none', fontSize: '0.8125rem', cursor: 'pointer',
            background: 'var(--surface-secondary, #f5f5f5)',
            color: 'var(--color-text-primary)',
          }}
        >
          ✕
        </button>
      </div>
    );
  }

  return (
    <button
      onClick={() => {
        setTitle(defaultTitle);
        setShowForm(true);
      }}
      style={{
        padding: '0.375rem 0.75rem', borderRadius: '6px',
        border: '1px solid var(--border-subtle, #e0e0e0)',
        background: 'var(--surface-secondary, #f5f5f5)',
        color: 'var(--color-text-primary)',
        fontSize: '0.8125rem', cursor: 'pointer',
        transition: 'all 0.15s',
      }}
    >
      {label}
    </button>
  );
}
