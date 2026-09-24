/**
 * useGoToHotkeys.ts — Go to シーケンス フック
 *
 * G キーに続いて別のキーを押すことで画面遷移:
 *   G then M → /my-issues
 *   G then I → /team/:slug/tickets（または /my-issues）
 *   G then H → /team/:slug/dashboard
 *   G then P → /team/:slug/projects
 *   G then N → /notifications
 *   G then S → /settings
 *
 * 約1秒のタイムアウト付き
 */

import { useEffect, useCallback, useRef } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { getLastTeamSlug } from './useTeam';
import { hasCommandModifier } from './keyboardGuards';

const SEQUENCE_TIMEOUT = 1000; // ms

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

export function useGoToHotkeys() {
  const navigate = useNavigate();
  const location = useLocation();
  const sequenceRef = useRef<string>('');
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const getTeamSlug = useCallback((): string | null => {
    const match = location.pathname.match(/^\/team\/([^/]+)/);
    return match?.[1] ?? getLastTeamSlug() ?? null;
  }, [location.pathname]);

  const resetSequence = useCallback(() => {
    sequenceRef.current = '';
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
  }, []);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      // Cmd+G(検索の次へ)などを、Go to シーケンスと取り違えない
      if (hasCommandModifier(e) || isInputFocused()) return;

      if (e.key === 'g' || e.key === 'G') {
        e.preventDefault();
        sequenceRef.current = 'g';
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => {
          resetSequence();
        }, SEQUENCE_TIMEOUT);
        return;
      }

      if (sequenceRef.current !== 'g') return;

      e.preventDefault();
      const key = e.key.toLowerCase();

      switch (key) {
        case 'm':
          navigate('/my-issues');
          resetSequence();
          break;
        case 'i': {
          const teamSlug = getTeamSlug();
          navigate(teamSlug ? `/team/${teamSlug}/tickets` : '/my-issues');
          resetSequence();
          break;
        }
        case 'h': {
          const teamSlug = getTeamSlug();
          if (teamSlug) navigate(`/team/${teamSlug}/dashboard`);
          resetSequence();
          break;
        }
        case 'p': {
          const teamSlug = getTeamSlug();
          if (teamSlug) navigate(`/team/${teamSlug}/projects`);
          resetSequence();
          break;
        }
        case 'n':
          navigate('/notifications');
          resetSequence();
          break;
        case 's':
          navigate('/settings');
          resetSequence();
          break;
        default:
          resetSequence();
      }
    },
    [navigate, getTeamSlug, resetSequence],
  );

  useEffect(() => {
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, [handleKeyDown]);
}
