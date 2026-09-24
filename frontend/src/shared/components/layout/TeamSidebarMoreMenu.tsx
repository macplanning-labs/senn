import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { isPinnedSlug } from '@/shared/utils/sidebarTeamPins';

/** 縦三点（ミートボール）— 未固定時 */
function IconMoreVertical() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <circle cx="12" cy="5" r="2" />
      <circle cx="12" cy="12" r="2" />
      <circle cx="12" cy="19" r="2" />
    </svg>
  );
}

/** ピン — 固定済み時に表示 */
function IconPin() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 17v5" />
      <path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6a3 3 0 1 0-6 0z" />
    </svg>
  );
}

interface TeamSidebarMoreMenuProps {
  teamSlug: string;
  pinnedSlugs: string[];
  onTogglePin: (slug: string) => void;
}

/**
 * チーム行右端の操作。
 * 未固定: 縦三点 → メニュー「サイドバーに固定」
 * 固定済: ピンアイコン → メニュー「固定を解除」
 * （参考: スペース一覧の PIN / オプション UI）
 */
export function TeamSidebarMoreMenu({
  teamSlug,
  pinnedSlugs,
  onTogglePin,
}: TeamSidebarMoreMenuProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);
  const pinned = isPinnedSlug(teamSlug, pinnedSlugs);

  useEffect(() => {
    if (!open) return;

    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    const onDown = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onDown);
    return () => {
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onDown);
    };
  }, [open]);

  return (
    <div className={`sidebar__team-more${pinned ? ' sidebar__team-more--pinned' : ''}`} ref={wrapRef}>
      <button
        type="button"
        className={`sidebar__team-more-btn${pinned ? ' sidebar__team-more-btn--pinned' : ''}`}
        aria-label={pinned ? t('sidebar.unpinTeam') : t('sidebar.teamMore')}
        aria-haspopup="menu"
        aria-expanded={open}
        title={pinned ? t('sidebar.unpinTeam') : t('sidebar.teamMore')}
        data-testid={`team-more-${teamSlug}`}
        data-pinned={pinned ? 'true' : 'false'}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setOpen((v) => !v);
        }}
      >
        {pinned ? <IconPin /> : <IconMoreVertical />}
      </button>
      {open && (
        <div
          role="menu"
          className="sidebar__team-more-menu"
          data-testid={`team-more-menu-${teamSlug}`}
        >
          <button
            type="button"
            role="menuitem"
            className="sidebar__team-more-item"
            data-testid={`team-pin-action-${teamSlug}`}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onTogglePin(teamSlug);
              setOpen(false);
            }}
          >
            {pinned ? t('sidebar.unpinTeam') : t('sidebar.pinTeam')}
          </button>
        </div>
      )}
    </div>
  );
}
