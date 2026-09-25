import { useEffect, useRef, useCallback } from 'react';
import { hasCommandModifier } from '@/shared/hooks/keyboardGuards';

const G_SEQUENCE_TIMEOUT = 1000;

export type TicketHotkeyAction =
  | 'status'
  | 'assignee'
  | 'priority'
  | 'type'
  | 'cycle'
  | 'project'
  | 'labels'
  | 'estimate'
  | 'dueDate'
  | 'focusTitle'
  | 'focusDescription';

function isInputFocused(): boolean {
  const active = document.activeElement;
  if (!active) return false;
  const tag = active.tagName.toLowerCase();
  return (
    tag === 'input' ||
    tag === 'textarea' ||
    tag === 'select' ||
    (active as HTMLElement).isContentEditable
  );
}

function isPickerActive(): boolean {
  if (document.querySelector('[data-ticket-field-picker-open]')) return true;
  const active = document.activeElement;
  if (!active) return false;
  if (active.matches('[cmdk-input]')) return true;
  return !!active.closest('.ticket-field-picker');
}

export function useTicketDetailHotkeys(opts: {
  enabled: boolean;
  onAction: (action: TicketHotkeyAction) => void;
}): void {
  const { enabled, onAction } = opts;
  const onActionRef = useRef(onAction);
  onActionRef.current = onAction;

  const gWaitingRef = useRef(false);
  const gTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearGWait = useCallback(() => {
    gWaitingRef.current = false;
    if (gTimeoutRef.current) {
      clearTimeout(gTimeoutRef.current);
      gTimeoutRef.current = null;
    }
  }, []);

  const startGWait = useCallback(() => {
    gWaitingRef.current = true;
    if (gTimeoutRef.current) clearTimeout(gTimeoutRef.current);
    gTimeoutRef.current = setTimeout(() => {
      gWaitingRef.current = false;
      gTimeoutRef.current = null;
    }, G_SEQUENCE_TIMEOUT);
  }, []);

  useEffect(() => {
    if (!enabled) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (hasCommandModifier(e) || isInputFocused() || isPickerActive()) return;

      if (e.key === 'g' || e.key === 'G') {
        startGWait();
        return;
      }

      if (gWaitingRef.current) return;

      let action: TicketHotkeyAction | null = null;

      if (e.key === 'P' && e.shiftKey) {
        action = 'project';
      } else if (e.key === 'D' && e.shiftKey) {
        action = 'dueDate';
      } else if (e.key === 'i' || e.key === 'I') {
        action = 'focusTitle';
      } else if (e.key === 'd' && !e.shiftKey) {
        action = 'focusDescription';
      } else if (e.key === 'p' && !e.shiftKey) {
        action = 'priority';
      } else {
        switch (e.key) {
          case 's':
            action = 'status';
            break;
          case 't':
            action = 'type';
            break;
          case 'a':
            action = 'assignee';
            break;
          case 'c':
            action = 'cycle';
            break;
          case 'l':
            action = 'labels';
            break;
          case 'e':
            action = 'estimate';
            break;
          default:
            break;
        }
      }

      if (action) {
        e.preventDefault();
        onActionRef.current(action);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      clearGWait();
    };
  }, [enabled, startGWait, clearGWait]);
}
