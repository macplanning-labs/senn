/**
 * App.tsx — WIP アプリケーションルート
 *
 * React Router v7 によるルーティング。
 * 認証状態に応じてレイアウトを切り替え。
 */

import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MainLayout } from '@/shared/components/layout/MainLayout';
import { LoginForm } from '@/features/auth/components/LoginForm';
import { Dashboard } from '@/features/dashboard/components/Dashboard';
import { TicketTable } from '@/features/tickets/components/TicketTable';
import { TicketDetail } from '@/features/tickets/components/TicketDetail';
import { TicketForm } from '@/features/tickets/components/TicketForm';
import { GanttChart } from '@/features/gantt/components/GanttChart';
import { WikiList } from '@/features/wiki/components/WikiList';
import { CommandPalette } from '@/shared/components/ui/CommandPalette';
import { useAuthStore } from '@/shared/stores/authStore';
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

/** プレースホルダーページ（Week 2で実装） */
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
        Coming in Week 2...
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
            <Route path="/dashboard" element={<Dashboard />} />
            <Route path="/tickets" element={<TicketTable />} />
            <Route path="/tickets/new" element={<TicketForm />} />
            <Route path="/tickets/:id/edit" element={<TicketForm />} />
            <Route path="/tickets/:id" element={<TicketDetail />} />
            <Route path="/gantt" element={<GanttChart />} />
            <Route path="/wiki" element={<WikiList />} />
            <Route path="/notifications" element={<PlaceholderPage title="Notifications" />} />
            <Route path="/settings" element={<PlaceholderPage title="Settings" />} />
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
      </BrowserRouter>
    </QueryClientProvider>
  );
}
