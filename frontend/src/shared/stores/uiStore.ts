/**
 * uiStore.ts — グローバルUI状態管理
 *
 * テーマ切替、言語切替、サイドバー開閉など
 * アプリ全体のUI状態を管理するZustandストア。
 */

import { create } from 'zustand';
import { persist } from 'zustand/middleware';

type Theme = 'dark' | 'light';
type Language = 'en' | 'ja';

interface UIState {
  /** 現在のテーマ */
  theme: Theme;
  /** 現在の言語 */
  language: Language;
  /** サイドバーの開閉状態 */
  sidebarOpen: boolean;
  /** コマンドパレットの開閉状態 */
  commandPaletteOpen: boolean;
  /** チケット作成モーダルの開閉状態 */
  ticketFormModalOpen: boolean;
  /** モーダルでチケットを作成する対象プロジェクトキー */
  ticketFormModalProjectKey: string | null;
  /** モーダルでチケットを作成する対象TeamSlug */
  ticketFormModalTeamSlug: string | null;
  /** モーダルでチケット作成時、descriptionの初期値（コメントから新規チケット作成する場合など） */
  ticketFormModalInitialDescription: string | null;
  /** モーダルでチケット作成時、parentの初期値（コメントからサブチケット作成する場合など） */
  ticketFormModalInitialParent: number | null;

  // アクション
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
  setLanguage: (language: Language) => void;
  toggleSidebar: () => void;
  setCommandPaletteOpen: (open: boolean) => void;
  openTicketFormModal: (
    projectKey?: string | null,
    teamSlug?: string | null,
    options?: { initialDescription?: string; initialParent?: number | null },
  ) => void;
  closeTicketFormModal: () => void;
}

export const useUIStore = create<UIState>()(
  persist(
    (set) => ({
      theme: 'dark',
      language: navigator.language.startsWith('ja') ? 'ja' : 'en',
      sidebarOpen: true,
      commandPaletteOpen: false,
      ticketFormModalOpen: false,
      ticketFormModalProjectKey: null,
      ticketFormModalTeamSlug: null,
      ticketFormModalInitialDescription: null,
      ticketFormModalInitialParent: null,

      setTheme: (theme) => {
        document.documentElement.setAttribute('data-theme', theme);
        set({ theme });
      },
      toggleTheme: () =>
        set((state) => {
          const newTheme = state.theme === 'dark' ? 'light' : 'dark';
          document.documentElement.setAttribute('data-theme', newTheme);
          return { theme: newTheme };
        }),
      setLanguage: (language) => set({ language }),
      toggleSidebar: () => set((state) => ({ sidebarOpen: !state.sidebarOpen })),
      setCommandPaletteOpen: (open) => set({ commandPaletteOpen: open }),
      openTicketFormModal: (projectKey, teamSlug, options) =>
        set({
          ticketFormModalOpen: true,
          ticketFormModalProjectKey: projectKey ?? null,
          ticketFormModalTeamSlug: teamSlug ?? null,
          ticketFormModalInitialDescription: options?.initialDescription ?? null,
          ticketFormModalInitialParent: options?.initialParent ?? null,
        }),
      closeTicketFormModal: () =>
        set({
          ticketFormModalOpen: false,
          ticketFormModalProjectKey: null,
          ticketFormModalTeamSlug: null,
          ticketFormModalInitialDescription: null,
          ticketFormModalInitialParent: null,
        }),
    }),
    {
      name: 'wip-ui-preferences',
      partialize: (state) => ({
        theme: state.theme,
        language: state.language,
        sidebarOpen: state.sidebarOpen,
      }),
      onRehydrateStorage: () => (state) => {
        // localStorage から復元されたテーマを DOM に適用
        if (state?.theme) {
          document.documentElement.setAttribute('data-theme', state.theme);
        }
      },
    },
  ),
);
