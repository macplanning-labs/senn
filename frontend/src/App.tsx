/**
 * App.tsx — WIP アプリケーションルート
 *
 * React Router v7 によるルーティング。
 * プロジェクトスコープURL: /p/:projectKey/tickets 等
 * グローバルURL: /dashboard, /settings
 */

import { BrowserRouter, Routes, Route, Navigate, useNavigate } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MainLayout } from '@/shared/components/layout/MainLayout';
import { LoginForm } from '@/features/auth/components/LoginForm';
import { Dashboard } from '@/features/dashboard/components/Dashboard';
import { TicketListPage } from '@/features/tickets/components/TicketListPage';
import { TicketForm } from '@/features/tickets/components/TicketForm';
import { KanbanBoard } from '@/features/tickets/components/KanbanBoard';
import { GanttChart } from '@/features/gantt/components/GanttChart';
import { CycleList } from '@/features/cycles/components/CycleList';
import { CycleDetail } from '@/features/cycles/components/CycleDetail';
import { WikiList } from '@/features/wiki/components/WikiList';
import { SettingsPage } from '@/features/settings/components/SettingsPage';
import { ProjectSettingsPage } from '@/features/settings/components/ProjectSettingsPage';
import { NotificationsPage } from '@/features/notifications/components/NotificationsPage';
import { TeamsPage } from '@/features/teams/components/TeamsPage';
import { TriageRequestsPage } from '@/features/triage/components/TriageRequestsPage';
import { WorkloadReportPage } from '@/features/reports/components/WorkloadReportPage';
import { CommandPalette } from '@/shared/components/ui/CommandPalette';
import { ToastContainer } from '@/shared/components/ui/ToastContainer';
import { useAuthStore } from '@/shared/stores/authStore';
import { getLastProjectKey } from '@/shared/hooks/useProject';
import { useEffect } from 'react';

// TanStack Query クライアント
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 1000 * 60 * 5, // 5分間キャッシュ
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

/** 認証ガード — 未ログイン時はログインページにリダイレクト */
function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { isAuthenticated, isLoading } = useAuthStore();

  if (isLoading) {
    return (
      <div style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100vh',
        color: 'var(--color-text-tertiary)',
      }}>
        Loading...
      </div>
    );
  }

  if (!isAuthenticated) {
    return <Navigate to="/login" replace />;
  }

  return <>{children}</>;
}

/**
 * 旧URL → 新URL リダイレクト
 * /tickets → /p/:lastProjectKey/tickets
 * /wiki → /p/:lastProjectKey/wiki
 */
function RedirectToProject({ subpath }: { subpath: string }) {
  const navigate = useNavigate();
  const { isAuthenticated } = useAuthStore();

  useEffect(() => {
    if (!isAuthenticated) return;
    const lastKey = getLastProjectKey();
    if (lastKey) {
      navigate(`/p/${lastKey}/${subpath}`, { replace: true });
    } else {
      // プロジェクトキーがない場合、ダッシュボードへ
      navigate('/dashboard', { replace: true });
    }
  }, [isAuthenticated, navigate, subpath]);

  return null;
}

/** プロジェクトインデックス → tickets にリダイレクト */
function ProjectIndex() {
  return <TicketListPage />;
}

/** プレースホルダーページ */
function PlaceholderPage({ title }: { title: string }) {
  return (
    <div data-testid={`page-${title.toLowerCase()}`}>
      <h1 style={{
        fontSize: 'var(--font-size-2xl)',
        fontWeight: 'var(--font-weight-bold)',
        color: 'var(--color-text-primary)',
        marginBottom: 'var(--space-4)',
      }}>
        {title}
      </h1>
      <p style={{ color: 'var(--color-text-tertiary)' }}>
        Coming soon...
      </p>
    </div>
  );
}

export default function App() {
  const { fetchUser, isAuthenticated } = useAuthStore();

  // 起動時にユーザー情報を取得
  useEffect(() => {
    void fetchUser();
  }, [fetchUser]);

  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          {/* 認証ページ（レイアウトなし） */}
          <Route path="/login" element={<LoginForm />} />
          <Route path="/register" element={<PlaceholderPage title="Register" />} />

          {/* メインアプリ（サイドバー付きレイアウト） */}
          <Route
            element={
              <ProtectedRoute>
                <MainLayout />
              </ProtectedRoute>
            }
          >
            {/* グローバルページ */}
            <Route path="/dashboard" element={<Dashboard />} />
            <Route path="/teams" element={<TeamsPage />} />
            <Route path="/triage" element={<TriageRequestsPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="/notifications" element={<NotificationsPage />} />
            <Route path="/reports" element={<WorkloadReportPage />} />

            {/* プロジェクトスコープ */}
            <Route path="/p/:projectKey">
              <Route index element={<ProjectIndex />} />
              <Route path="tickets" element={<TicketListPage />} />
              <Route path="tickets/new" element={<TicketForm />} />
              <Route path="tickets/:ticketId/edit" element={<TicketForm />} />
              <Route path="tickets/:ticketId" element={<TicketListPage />} />
              <Route path="board" element={<KanbanBoard />} />
              <Route path="board/:ticketId" element={<KanbanBoard />} />
              <Route path="wiki" element={<WikiList />} />
              <Route path="gantt" element={<GanttChart />} />
              <Route path="cycles" element={<CycleList />} />
              <Route path="cycles/:cycleId" element={<CycleDetail />} />
              <Route path="settings" element={<ProjectSettingsPage />} />
            </Route>

            {/* 旧URL後方互換リダイレクト */}
            <Route path="/tickets" element={<RedirectToProject subpath="tickets" />} />
            <Route path="/tickets/*" element={<RedirectToProject subpath="tickets" />} />
            <Route path="/wiki" element={<RedirectToProject subpath="wiki" />} />
            <Route path="/gantt" element={<RedirectToProject subpath="gantt" />} />
          </Route>

          {/* ルートリダイレクト */}
          <Route
            path="/"
            element={
              isAuthenticated ? (
                <Navigate to="/dashboard" replace />
              ) : (
                <Navigate to="/login" replace />
              )
            }
          />

          {/* 404 */}
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
        <CommandPalette />
        <ToastContainer />
      </BrowserRouter>
    </QueryClientProvider>
  );
}
