import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { searchTicketsLocal } from '@/shared/sync/repos/ticketRepo';
import type { LocalTicket } from '@/shared/sync/db';
import type { RelationCreateKind } from '../utils/classifyDependencies';

interface TicketRelationAddFormProps {
  ticketKey: string;
  ticketId: number;
  open: boolean;
  defaultKind?: RelationCreateKind;
  kindLocked?: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (kind: RelationCreateKind, other: { id: number; ticketKey: string }) => Promise<void>;
  isSubmitting: boolean;
}

const KIND_OPTIONS: RelationCreateKind[] = ['blocking', 'blockedBy', 'related'];

export function TicketRelationAddForm({
  ticketKey,
  ticketId,
  open,
  defaultKind = 'related',
  kindLocked = false,
  onOpenChange,
  onSubmit,
  isSubmitting,
}: TicketRelationAddFormProps) {
  const { t } = useTranslation();
  const [kind, setKind] = useState<RelationCreateKind>(defaultKind);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<LocalTicket[]>([]);
  const [selected, setSelected] = useState<LocalTicket | null>(null);

  useEffect(() => {
    if (!open) {
      setKind(defaultKind);
      setQuery('');
      setResults([]);
      setSelected(null);
    } else {
      setKind(defaultKind);
    }
  }, [open, defaultKind]);

  useEffect(() => {
    if (!open) return;
    const q = query.trim();
    if (q.length < 1) {
      setResults([]);
      return;
    }
    const timer = window.setTimeout(() => {
      void searchTicketsLocal(q, 8).then((rows) =>
        setResults(rows.filter((r) => r.ticketKey !== ticketKey && r.id !== ticketId)),
      );
    }, 200);
    return () => window.clearTimeout(timer);
  }, [open, query, ticketKey, ticketId]);

  const kindLabel = useMemo(
    () => ({
      blocking: t('ticketDetail.relationTypeBlocking'),
      blockedBy: t('ticketDetail.relationTypeBlockedBy'),
      related: t('ticketDetail.relationTypeRelated'),
    }),
    [t],
  );

  if (!open) return null;

  const handleSubmit = async () => {
    if (!selected || isSubmitting) return;
    await onSubmit(kind, { id: selected.id, ticketKey: selected.ticketKey });
    onOpenChange(false);
  };

  return (
    <div className="ticket-relations__add-form" data-testid="ticket-relation-add-form">
      {!kindLocked && (
        <label className="ticket-relations__add-label">
          {t('ticketDetail.relationKind')}
          <select
            className="ticket-relations__add-select"
            value={kind}
            onChange={(e) => setKind(e.target.value as RelationCreateKind)}
            data-testid="ticket-relation-kind"
          >
            {KIND_OPTIONS.map((k) => (
              <option key={k} value={k}>
                {kindLabel[k]}
              </option>
            ))}
          </select>
        </label>
      )}

      <label className="ticket-relations__add-label">
        {t('ticketDetail.searchTicket')}
        <input
          type="search"
          className="ticket-relations__add-search"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setSelected(null);
          }}
          placeholder={t('ticketDetail.searchTicketPlaceholder')}
          data-testid="ticket-relation-search"
        />
      </label>

      {results.length > 0 && (
        <ul className="ticket-relations__search-results" data-testid="ticket-relation-search-results">
          {results.map((row) => (
            <li key={row.ticketKey}>
              <button
                type="button"
                className={`ticket-relations__search-item${
                  selected?.ticketKey === row.ticketKey ? ' ticket-relations__search-item--selected' : ''
                }`}
                onClick={() => setSelected(row)}
              >
                <span className="ticket-relations__search-key">{row.ticketKey}</span>
                <span className="ticket-relations__search-title">{row.title}</span>
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="ticket-relations__add-actions">
        <button
          type="button"
          className="ticket-relations__add-cancel"
          onClick={() => onOpenChange(false)}
          disabled={isSubmitting}
        >
          {t('common.cancel')}
        </button>
        <button
          type="button"
          className="ticket-relations__add-submit"
          onClick={() => void handleSubmit()}
          disabled={!selected || isSubmitting}
          data-testid="ticket-relation-submit"
        >
          {t('ticketDetail.addRelation')}
        </button>
      </div>
    </div>
  );
}
