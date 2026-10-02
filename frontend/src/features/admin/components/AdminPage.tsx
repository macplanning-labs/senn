/**
 * AdminPage.tsx — システム管理 (/admin) staff 専用
 */

import { useState } from 'react';
import { Navigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useAuthStore } from '@/shared/stores/authStore';
import { SystemMailSettings } from './SystemMailSettings';
import { AdminAiSection } from './AdminAiSection';
import { AdminBackupSection } from './AdminBackupSection';
import { AdminAccessSection } from './AdminAccessSection';
import './AdminPage.css';

type AdminSection = 'system' | 'ai' | 'access' | 'backup';

export function AdminPage() {
  const { t } = useTranslation();
  const { user, isLoading } = useAuthStore();
  const [section, setSection] = useState<AdminSection>('system');

  if (isLoading) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '50vh', color: 'var(--color-text-tertiary)' }}>
        Loading...
      </div>
    );
  }

  if (!user?.isSystemAdmin) {
    return <Navigate to="/my-issues" replace />;
  }

  return (
    <div className="admin" data-testid="admin-page">
      <header className="admin__header">
        <h1 className="admin__title">🏢 {t('admin.pageTitle')}</h1>
        <p className="admin__subtitle">
          {t('admin.pageSubtitle')}
        </p>
      </header>

      <div className="admin__layout">
        <nav className="admin__nav" aria-label={t('admin.navAriaLabel')}>
          <button
            type="button"
            className={`admin__nav-btn ${section === 'system' ? 'admin__nav-btn--active' : ''}`}
            onClick={() => setSection('system')}
          >
            {t('admin.systemSettingsTitle')}
          </button>
          <button
            type="button"
            className={`admin__nav-btn ${section === 'ai' ? 'admin__nav-btn--active' : ''}`}
            onClick={() => setSection('ai')}
          >
            {t('admin.navAiSettings')}
          </button>
          <button
            type="button"
            className={`admin__nav-btn ${section === 'access' ? 'admin__nav-btn--active' : ''}`}
            onClick={() => setSection('access')}
            data-testid="admin-nav-access"
          >
            {t('teamAccess.adminSection')}
          </button>
          <button
            type="button"
            className={`admin__nav-btn ${section === 'backup' ? 'admin__nav-btn--active' : ''}`}
            onClick={() => setSection('backup')}
          >
            {t('admin.backupTitle')}
          </button>
        </nav>

        <div className="admin__content">
          {section === 'system' && <SystemMailSettings />}
          {section === 'ai' && <AdminAiSection />}
          {section === 'access' && <AdminAccessSection />}
          {section === 'backup' && <AdminBackupSection />}
        </div>
      </div>
    </div>
  );
}
