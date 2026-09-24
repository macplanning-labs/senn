/**
 * TeamSectionMenu.tsx — サイドバーのチーム見出し「＋」の右にある「…」メニュー
 *
 * - 「👥 チームを管理」: チーム一覧（/teams）へ。チーム画面の「← チーム一覧」を廃止した代わりの入口
 * - 「⚙️ {チーム名}」: 各チームの設定（/team/:slug/settings）へ
 * キーボード: ↑↓ で項目を移動、Enter で開く、Esc で閉じてボタンへフォーカスを戻す
 */

import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { IconMoreHorizontal } from '@/shared/components/ui/icons';

interface TeamSectionMenuTeam {
  slug: string;
  name: string;
}

interface TeamSectionMenuProps {
  teams: TeamSectionMenuTeam[];
}

export function TeamSectionMenu({ teams }: TeamSectionMenuProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    // 開いたら先頭の項目へフォーカス（キーボードだけで操作できるように）
    menuRef.current?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus();

    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen(false);
        buttonRef.current?.focus();
        return;
      }
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        const items = Array.from(
          menuRef.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [],
        );
        if (items.length === 0) return;
        e.preventDefault();
        const current = items.indexOf(document.activeElement as HTMLButtonElement);
        const next =
          e.key === 'ArrowDown'
            ? (current + 1) % items.length
            : (current - 1 + items.length) % items.length;
        items[next]?.focus();
      }
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

  const go = (path: string) => {
    setOpen(false);
    navigate(path);
  };

  return (
    <div className="sidebar__team-more sidebar__section-more" ref={wrapRef}>
      <button
        ref={buttonRef}
        type="button"
        className="sidebar__section-add-btn sidebar__section-more-btn"
        aria-label={t('sidebar.teamSectionMenu')}
        title={t('sidebar.teamSectionMenu')}
        aria-haspopup="menu"
        aria-expanded={open}
        data-testid="team-section-more"
        onClick={() => setOpen((v) => !v)}
      >
        <IconMoreHorizontal />
      </button>
      {open && (
        <div
          ref={menuRef}
          role="menu"
          className="sidebar__team-more-menu sidebar__section-more-menu"
          data-testid="team-section-menu"
        >
          <button
            type="button"
            role="menuitem"
            className="sidebar__team-more-item"
            data-testid="team-section-menu-manage"
            onClick={() => go('/teams')}
          >
            👥 {t('sidebar.teamSectionManage')}
          </button>
          {teams.length > 0 && (
            <>
              <div className="sidebar__section-more-divider" role="separator" />
              <div className="sidebar__section-more-heading" aria-hidden="true">
                {t('sidebar.teamSettingsHeading')}
              </div>
              {teams.map((team) => (
                <button
                  key={team.slug}
                  type="button"
                  role="menuitem"
                  className="sidebar__team-more-item"
                  data-testid={`team-section-menu-settings-${team.slug}`}
                  onClick={() => go(`/team/${team.slug}/settings`)}
                >
                  ⚙️ {team.name}
                </button>
              ))}
            </>
          )}
        </div>
      )}
    </div>
  );
}
