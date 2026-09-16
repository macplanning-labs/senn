/**
 * AdminPage.tsx — システム管理 (/admin) staff 専用
 */

import { useState } from 'react';
import { Navigate } from 'react-router-dom';
import { useAuthStore } from '@/shared/stores/authStore';
import { SystemMailSettings } from './SystemMailSettings';
import { AdminAiSection } from './AdminAiSection';
import { AdminBackupSection } from './AdminBackupSection';
import './AdminPage.css';

type AdminSection = 'system' | 'ai' | 'backup';

export function AdminPage() {
  const { user, isLoading } = useAuthStore();
  const [section, setSection] = useState<AdminSection>('system');

  if (isLoading) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '50vh', color: 'var(--color-text-tertiary)' }}>
        Loading...
      </div>
    );
  }

  if (!user?.isStaff) {
    return <Navigate to="/my-issues" replace />;
  }

  return (
    <div className="admin" data-testid="admin-page">
      <header className="admin__header">
        <h1 className="admin__title">🏢 システム管理</h1>
        <p className="admin__subtitle">
          インスタンス全体の設定（個人設定 /settings とは別 URL）
        </p>
      </header>

      <div className="admin__layout">
        <nav className="admin__nav" aria-label="システム管理メニュー">
          <button
            type="button"
            className={`admin__nav-btn ${section === 'system' ? 'admin__nav-btn--active' : ''}`}
            onClick={() => setSection('system')}
          >
            全体システム設定
          </button>
          <button
            type="button"
            className={`admin__nav-btn ${section === 'ai' ? 'admin__nav-btn--active' : ''}`}
            onClick={() => setSection('ai')}
          >
            AI 設定
          </button>
          <button
            type="button"
            className={`admin__nav-btn ${section === 'backup' ? 'admin__nav-btn--active' : ''}`}
            onClick={() => setSection('backup')}
          >
            バックアップ
          </button>
        </nav>

        <div className="admin__content">
          {section === 'system' && <SystemMailSettings />}
          {section === 'ai' && <AdminAiSection />}
          {section === 'backup' && <AdminBackupSection />}
        </div>
      </div>
    </div>
  );
}
