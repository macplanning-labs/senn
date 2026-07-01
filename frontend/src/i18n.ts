/**
 * i18n.ts — WIP 国際化設定
 *
 * i18next + react-i18next による日英バイリンガル対応。
 * 翻訳ファイルは public/locales/{lang}.json に配置。
 */

import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

// 翻訳リソース（インライン定義。ファイルが大きくなったら分割）
const resources = {
  en: {
    translation: {
      app: {
        name: 'WIP',
        tagline: 'Project management that never lags.',
      },
      nav: {
        dashboard: 'Dashboard',
        tickets: 'Tickets',
        board: 'Board',
        cycles: 'Cycles',
        gantt: 'Gantt Chart',
        wiki: 'Wiki',
        teams: 'Teams',
        triage: 'Triage',
        notifications: 'Notifications',
        settings: 'Settings',
      },
      sidebar: {
        selectProject: 'Select project',
        noProjects: 'No projects found',
        newProject: 'New project',
        projectName: 'Project name',
        createValidation: 'Enter name and prefix',
        createError: 'Failed to create',
        project: 'Project',
        global: 'Global',
        lightMode: 'Light mode',
        darkMode: 'Dark mode',
      },
      auth: {
        login: 'Log in',
        logout: 'Log out',
        register: 'Sign up',
        email: 'Email',
        password: 'Password',
        forgotPassword: 'Forgot password?',
      },
      ticket: {
        create: 'Create Ticket',
        edit: 'Edit Ticket',
        delete: 'Delete Ticket',
        title: 'Title',
        description: 'Description',
        assignee: 'Assignee',
        priority: 'Priority',
        dueDate: 'Due Date',
        status: {
          open: 'Open',
          in_progress: 'In Progress',
          resolved: 'Resolved',
          closed: 'Closed',
        },
        priority_label: {
          urgent: 'Urgent',
          high: 'High',
          medium: 'Medium',
          low: 'Low',
        },
      },
      dashboard: {
        title: 'Dashboard',
        myTickets: 'My Tickets',
        overdue: 'Overdue',
        totalProjects: 'Projects',
        openTickets: 'Open Tickets',
        completedThisWeek: 'Completed This Week',
      },
      wiki: {
        create: 'New Page',
        edit: 'Edit Page',
        revisions: 'Revisions',
      },
      common: {
        save: 'Save',
        cancel: 'Cancel',
        delete: 'Delete',
        confirm: 'Confirm',
        search: 'Search...',
        noResults: 'No results found',
        loading: 'Loading...',
        error: 'An error occurred',
        retry: 'Retry',
      },
    },
  },
  ja: {
    translation: {
      app: {
        name: 'WIP',
        tagline: '止まらないプロジェクト管理。',
      },
      nav: {
        dashboard: 'ダッシュボード',
        tickets: 'チケット',
        board: 'ボード',
        cycles: 'サイクル',
        gantt: 'ガントチャート',
        wiki: 'Wiki',
        teams: 'チーム',
        triage: 'トリアージ',
        notifications: '通知',
        settings: '設定',
      },
      sidebar: {
        selectProject: 'プロジェクトを選択',
        noProjects: 'プロジェクトがありません',
        newProject: '新規プロジェクト',
        projectName: 'プロジェクト名',
        createValidation: '名前とプレフィックスを入力してください',
        createError: '作成に失敗しました',
        project: 'プロジェクト',
        global: 'グローバル',
        lightMode: 'ライトモード',
        darkMode: 'ダークモード',
      },
      auth: {
        login: 'ログイン',
        logout: 'ログアウト',
        register: '新規登録',
        email: 'メールアドレス',
        password: 'パスワード',
        forgotPassword: 'パスワードを忘れた方',
      },
      ticket: {
        create: 'チケット作成',
        edit: 'チケット編集',
        delete: 'チケット削除',
        title: 'タイトル',
        description: '説明',
        assignee: '担当者',
        priority: '優先度',
        dueDate: '期限',
        status: {
          open: '未対応',
          in_progress: '対応中',
          resolved: '解決済み',
          closed: '完了',
        },
        priority_label: {
          urgent: '緊急',
          high: '高',
          medium: '中',
          low: '低',
        },
      },
      dashboard: {
        title: 'ダッシュボード',
        myTickets: '自分のチケット',
        overdue: '期限超過',
        totalProjects: 'プロジェクト',
        openTickets: '未完了チケット',
        completedThisWeek: '今週の完了',
      },
      wiki: {
        create: '新規ページ',
        edit: 'ページ編集',
        revisions: 'リビジョン',
      },
      common: {
        save: '保存',
        cancel: 'キャンセル',
        delete: '削除',
        confirm: '確認',
        search: '検索...',
        noResults: '結果が見つかりません',
        loading: '読み込み中...',
        error: 'エラーが発生しました',
        retry: 'やり直す',
      },
    },
  },
};

i18n.use(initReactI18next).init({
  resources,
  lng: navigator.language.startsWith('ja') ? 'ja' : 'en',
  fallbackLng: 'en',
  interpolation: {
    escapeValue: false, // React は自動エスケープ
  },
});

export default i18n;
