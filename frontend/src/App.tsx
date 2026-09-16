/**
 * App.tsx — WIP アプリケーションルート
 *
 * React Router v7 によるルーティング。
 * プロジェクトスコープURL: /project/:projectKey/tickets 等
 * チームスコープURL: /team/:teamSlug/... / グローバル: /dashboard, /settings
 */

import { BrowserRouter, Routes, Route, Navigate, useNavigate, useLocation } from 'react-router-dom';
import { QueryClient, QueryClientProvider, MutationCache } from '@tanstack/react-query';
import { MainLayout } from '@/shared/components/layout/MainLayout';
import { LoginForm } from '@/features/auth/components/LoginForm';
import { RegisterForm } from '@/features/auth/components/RegisterForm';
import { ForgotPasswordPage } from '@/features/auth/components/ForgotPasswordPage';
import { ResetPasswordPage } from '@/features/auth/components/ResetPasswordPage';
import { Dashboard } from '@/features/dashboard/components/Dashboard';
import { TeamDashboard } from '@/features/dashboard/components/TeamDashboard';
import { TicketListPage } from '@/features/tickets/components/TicketListPage';
import { MyIssuesPage } from '@/features/tickets/components/MyIssuesPage';
import { TicketForm } from '@/features/tickets/components/TicketForm';
import { KanbanBoard } from '@/features/tickets/components/KanbanBoard';
import { GanttChart } from '@/features/gantt/components/GanttChart';
import { TaskDependencyFlow } from '@/features/dependencies/components/TaskDependencyFlow';
import { CycleList } from '@/features/cycles/components/CycleList';
import { CycleDetail } from '@/features/cycles/components/CycleDetail';
import { WikiList } from '@/features/wiki/components/WikiList';
import { SettingsPage } from '@/features/settings/components/SettingsPage';
import { ProjectSettingsPage } from '@/features/settings/components/ProjectSettingsPage';
import { TeamSettingsPage } from '@/features/settings/components/TeamSettingsPage';
import { NotificationsPage } from '@/features/notifications/components/NotificationsPage';
import { AdminPage } from '@/features/admin/components/AdminPage';
import { TeamsPage } from '@/features/teams/components/TeamsPage';
import { TeamProjectsPage } from '@/features/teams/components/TeamProjectsPage';
import { TriageRequestsPage } from '@/features/triage/components/TriageRequestsPage';
import { WorkloadReportPage } from '@/features/reports/components/WorkloadReportPage';
import { CommandPalette } from '@/shared/components/ui/CommandPalette';
import { TicketFormModal } from '@/features/tickets/components/TicketFormModal';
import { GeneratePromptModal } from '@/features/tickets/components/GeneratePromptModal';
import { ToastContainer } from '@/shared/components/ui/ToastContainer';
import { useAuthStore } from '@/shared/stores/authStore';
import { getLastProjectKey } from '@/shared/hooks/useProject';
import { useToastStore } from '@/shared/stores/toastStore';
import { useEffect } from 'react';
import type { AxiosError } from 'axios';

// グローバル MutationCache — 全 mutation のエラーをトースト表示
const mutationCache = new MutationCache({
  onError: (error: Error) => {
    const axiosError = error as AxiosError<{ detail?: string }>;
    const detail =
      axiosError.response?.data?.detail ??
      axiosError.message ??
      'エラーが発生しました';
    useToastStore.getState().addToast({ type: 'error', message: detail });
  },
});

// TanStack Query クライアント
const queryClient = new QueryClient({
  mutationCache,
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
 * /tickets → /project/:lastProjectKey/tickets
 * /wiki → /project/:lastProjectKey/wiki
 */

/** 旧 /p/* → /project/* 、旧 /t/* → /team/* */
function LegacyPrefixRedirect({ from, to }: { from: 'p' | 't'; to: 'project' | 'team' }) {
  const location = useLocation();
  const rest = location.pathname.slice(from.length + 1); // "/p".length==2 → drop prefix
  const dest = `/${to}${rest}${location.search}${location.hash}`;
  return <Navigate to={dest} replace />;
}

function RedirectToProject({ subpath }: { subpath: string }) {
  const navigate = useNavigate();
  const { isAuthenticated } = useAuthStore();

  useEffect(() => {
    if (!isAuthenticated) return;
    const lastKey = getLastProjectKey();
    if (lastKey) {
      navigate(`/project/${lastKey}/${subpath}`, { replace: true });
    } else {
      navigate('/my-issues', { replace: true });
    }
  }, [isAuthenticated, navigate, subpath]);

  return null;
}

/** プロジェクトインデックス → tickets にリダイレクト */
function ProjectIndex() {
  return <TicketListPage />;
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
          <Route path="/forgot-password" element={<ForgotPasswordPage />} />
          <Route path="/reset-password" element={<ResetPasswordPage />} />
          <Route path="/register" element={<RegisterForm />} />

          {/* メインアプリ（サイドバー付きレイアウト） */}
          <Route
            element={
              <ProtectedRoute>
                <MainLayout />
              </ProtectedRoute>
            }
          >
            {/* グローバルページ */}
            <Route path="/my-issues" element={<MyIssuesPage />} />
            <Route path="/my-issues/:ticketId" element={<MyIssuesPage />} />
            <Route path="/dashboard" element={<Dashboard />} />
            <Route path="/teams" element={<TeamsPage />} />
            <Route path="/triage" element={<TriageRequestsPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="/admin" element={<AdminPage />} />
            <Route path="/notifications" element={<NotificationsPage />} />
            <Route path="/reports" element={<WorkloadReportPage />} />

            {/* プロジェクトスコープ */}
            <Route path="/project/:projectKey">
              <Route index element={<ProjectIndex />} />
              <Route path="tickets" element={<TicketListPage />} />
              <Route path="tickets/new" element={<TicketForm />} />
              <Route path="tickets/:ticketId/edit" element={<TicketForm />} />
              <Route path="tickets/:ticketId" element={<TicketListPage />} />
              <Route path="board" element={<KanbanBoard />} />
              <Route path="board/:ticketId" element={<KanbanBoard />} />
              <Route path="wiki" element={<WikiList />} />
              <Route path="gantt" element={<GanttChart />} />
              <Route path="dependencies" element={<TaskDependencyFlow />} />
              <Route path="cycles" element={<CycleList />} />
              <Route path="cycles/:cycleId/:ticketId" element={<CycleDetail />} />
              <Route path="cycles/:cycleId" element={<CycleDetail />} />
              <Route path="settings" element={<ProjectSettingsPage />} />
            </Route>

            {/* Teamスコープ（Team-onlyチケット） */}
            <Route path="/team/:teamSlug">
              <Route path="tickets" element={<TicketListPage />} />
              <Route path="tickets/new" element={<TicketForm />} />
              <Route path="tickets/:ticketId/edit" element={<TicketForm />} />
              <Route path="tickets/:ticketId" element={<TicketListPage />} />
              <Route path="board" element={<KanbanBoard />} />
              <Route path="board/:ticketId" element={<KanbanBoard />} />
              <Route path="gantt" element={<GanttChart />} />
              <Route path="wiki" element={<WikiList />} />
              <Route path="dependencies" element={<TaskDependencyFlow />} />
              <Route path="cycles" element={<CycleList />} />
              <Route path="cycles/:cycleId/:ticketId" element={<CycleDetail />} />
              <Route path="cycles/:cycleId" element={<CycleDetail />} />
              <Route path="dashboard" element={<TeamDashboard />} />
              <Route path="projects" element={<TeamProjectsPage />} />
              <Route path="settings" element={<TeamSettingsPage />} />
            </Route>

            {/* 旧URL後方互換リダイレクト（/p → /project, /t → /team） */}
            <Route path="/p/*" element={<LegacyPrefixRedirect from="p" to="project" />} />
            <Route path="/t/*" element={<LegacyPrefixRedirect from="t" to="team" />} />
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
                <Navigate to="/my-issues" replace />
              ) : (
                <Navigate to="/login" replace />
              )
            }
          />

          {/* 404 */}
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
        <CommandPalette />
        <TicketFormModal />
        <GeneratePromptModal />
        <ToastContainer />
      </BrowserRouter>
    </QueryClientProvider>
  );
}
