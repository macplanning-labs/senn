import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { Command } from 'cmdk';
import './TicketFieldPicker.css';

export type TicketPickerField =
  | 'type'
  | 'status'
  | 'assignee'
  | 'priority'
  | 'cycle'
  | 'project'
  | 'labels'
  | 'estimate'
  | 'dueDate'
  | 'startDate'
  | 'category'
  | 'milestone'
  | 'reviewer';

export interface TicketFieldPickerOption {
  id: string;
  label: string;
  /** optional secondary text */
  hint?: string;
}

export interface TicketFieldPickerProps {
  field: TicketPickerField;
  open: boolean;
  onClose: () => void;
  /** status → center modal; others → anchored near rect */
  placement: 'center' | 'anchor';
  anchorRect: DOMRect | null;
  options: TicketFieldPickerOption[];
  /** multi-select for assignee/labels/reviewer */
  multi?: boolean;
  selectedIds: string[];
  onConfirm: (selectedIds: string[]) => void;
  loading?: boolean;
  /** show "Unassigned"/empty option with id "0" */
  allowEmpty?: boolean;
  emptyLabel?: string;
  title: string;
  /** date fields: show native date input instead of option list */
  dateMode?: boolean;
  dateValue?: string | null;
  onConfirmDate?: (value: string | null) => void;
}

const PANEL_WIDTH = 320;
const PANEL_MAX_HEIGHT = 360;

function filterOptions(options: TicketFieldPickerOption[], query: string): TicketFieldPickerOption[] {
  const q = query.trim().toLowerCase();
  if (!q) return options;
  return options.filter(
    (opt) =>
      opt.label.toLowerCase().includes(q) ||
      opt.hint?.toLowerCase().includes(q) ||
      opt.id.toLowerCase().includes(q),
  );
}

function computeAnchorStyle(anchorRect: DOMRect | null): CSSProperties | undefined {
  if (!anchorRect) return undefined;
  const left = Math.min(
    Math.max(8, anchorRect.left),
    window.innerWidth - PANEL_WIDTH - 8,
  );
  const topBelow = anchorRect.bottom + 4;
  const topAbove = anchorRect.top - PANEL_MAX_HEIGHT - 4;
  const top =
    topBelow + PANEL_MAX_HEIGHT > window.innerHeight && topAbove >= 8
      ? topAbove
      : Math.min(topBelow, window.innerHeight - PANEL_MAX_HEIGHT - 8);
  return { top, left, width: PANEL_WIDTH };
}

export function TicketFieldPicker({
  field,
  open,
  onClose,
  placement,
  anchorRect,
  options,
  multi = false,
  selectedIds,
  onConfirm,
  loading = false,
  allowEmpty = false,
  emptyLabel = 'Unassigned',
  title,
  dateMode = false,
  dateValue = null,
  onConfirmDate,
}: TicketFieldPickerProps) {
  const [search, setSearch] = useState('');
  const [pendingIds, setPendingIds] = useState<string[]>(selectedIds);
  const [pendingDate, setPendingDate] = useState(dateValue ?? '');
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) {
      setSearch('');
      setPendingIds(selectedIds);
      setPendingDate(dateValue ?? '');
    }
  }, [open, selectedIds, dateValue]);

  useEffect(() => {
    if (!open) return;
    document.documentElement.setAttribute('data-ticket-field-picker-open', field);
    return () => {
      document.documentElement.removeAttribute('data-ticket-field-picker-open');
    };
  }, [open, field]);

  const allOptions = useMemo(() => {
    if (!allowEmpty) return options;
    return [{ id: '0', label: emptyLabel }, ...options];
  }, [allowEmpty, emptyLabel, options]);

  const visibleOptions = useMemo(
    () => filterOptions(allOptions, search),
    [allOptions, search],
  );

  const handleConfirm = useCallback(
    (ids: string[]) => {
      onConfirm(ids);
      onClose();
    },
    [onConfirm, onClose],
  );

  const handleSelectOption = useCallback(
    (id: string) => {
      if (multi) {
        setPendingIds((prev) =>
          prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
        );
        return;
      }
      handleConfirm([id]);
    },
    [multi, handleConfirm],
  );

  const handleConfirmMulti = useCallback(() => {
    handleConfirm(pendingIds);
  }, [handleConfirm, pendingIds]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
        return;
      }

      if (multi && e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleConfirmMulti();
        return;
      }

      if (search.trim() !== '') return;

      const digit = e.key >= '1' && e.key <= '9' ? Number(e.key) : null;
      if (digit === null || e.metaKey || e.ctrlKey || e.altKey) return;

      const index = digit - 1;
      const target = visibleOptions[index];
      if (!target) return;

      e.preventDefault();
      handleSelectOption(target.id);
    },
    [search, visibleOptions, multi, handleConfirmMulti, handleSelectOption, onClose],
  );

  useEffect(() => {
    if (!open) return;

    const onDocKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };

    document.addEventListener('keydown', onDocKeyDown);
    return () => document.removeEventListener('keydown', onDocKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  const anchorStyle = placement === 'anchor' ? computeAnchorStyle(anchorRect) : undefined;
  const placementClass =
    placement === 'center'
      ? 'ticket-field-picker--center'
      : 'ticket-field-picker--anchor';

  return (
    <>
      <div
        className="ticket-field-picker__overlay"
        onClick={onClose}
        data-testid={`ticket-field-picker-overlay-${field}`}
      />
      <div
        ref={panelRef}
        className={`ticket-field-picker ${placementClass}`}
        style={anchorStyle}
        data-testid={`ticket-field-picker-${field}`}
        onKeyDown={handleKeyDown}
      >
        <div className="ticket-field-picker__header">{title}</div>

        {dateMode ? (
          <div className="ticket-field-picker__date">
            <input
              type="date"
              className="ticket-field-picker__date-input"
              value={pendingDate}
              autoFocus
              onChange={(e) => setPendingDate(e.target.value)}
            />
            <div className="ticket-field-picker__footer">
              <button
                type="button"
                className="ticket-field-picker__done ticket-field-picker__done--ghost"
                onClick={() => {
                  onConfirmDate?.(null);
                  onClose();
                }}
              >
                Clear
              </button>
              <button
                type="button"
                className="ticket-field-picker__done"
                onClick={() => {
                  onConfirmDate?.(pendingDate || null);
                  onClose();
                }}
              >
                Done
              </button>
            </div>
          </div>
        ) : loading ? (
          <div className="ticket-field-picker__loading">Loading…</div>
        ) : (
          <Command label={title} shouldFilter={false}>
            <Command.Input
              className="ticket-field-picker__input"
              placeholder="Search…"
              autoFocus
              value={search}
              onValueChange={setSearch}
            />
            <Command.List className="ticket-field-picker__list">
              <Command.Empty className="ticket-field-picker__empty">
                No results
              </Command.Empty>
              {visibleOptions.map((opt) => {
                const isSelected = multi
                  ? pendingIds.includes(opt.id)
                  : selectedIds.includes(opt.id);
                return (
                  <Command.Item
                    key={opt.id}
                    value={`${opt.id} ${opt.label} ${opt.hint ?? ''}`}
                    onSelect={() => handleSelectOption(opt.id)}
                    className={`ticket-field-picker__item${isSelected ? ' ticket-field-picker__item--selected' : ''}`}
                  >
                    {multi && (
                      <span className="ticket-field-picker__check">
                        {isSelected ? '✓' : ''}
                      </span>
                    )}
                    <span className="ticket-field-picker__item-label">{opt.label}</span>
                    {opt.hint && (
                      <span className="ticket-field-picker__item-hint">{opt.hint}</span>
                    )}
                  </Command.Item>
                );
              })}
            </Command.List>
          </Command>
        )}

        {multi && !loading && !dateMode && (
          <div className="ticket-field-picker__footer">
            <button
              type="button"
              className="ticket-field-picker__done"
              onClick={handleConfirmMulti}
            >
              Done
            </button>
          </div>
        )}
      </div>
    </>
  );
}
