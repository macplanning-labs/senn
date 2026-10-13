// @vitest-environment jsdom
/**
 * AdminPasskeyBanner.test.tsx — 管理者のパスキー登録の案内
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { MemoryRouter } from 'react-router-dom';
import i18n from '@/i18n';
import { useAuthStore } from '@/shared/stores/authStore';
import { AdminPasskeyBanner } from './AdminPasskeyBanner';

// authStore は読み込み時に localStorage を読む。Node の新しい版(26 など)では jsdom の localStorage が
// 見えないことがあるため、他のテストと同じく代用品を先に用意する(import より前に実行される)
vi.hoisted(() => {
  const data: Record<string, string> = {};
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: {
      getItem: (k: string) => data[k] ?? null,
      setItem: (k: string, v: string) => {
        data[k] = v;
      },
      removeItem: (k: string) => {
        delete data[k];
      },
      clear: () => {
        for (const k of Object.keys(data)) delete data[k];
      },
    },
  });
});

function setAdminPasskey(adminPasskey: {
  isAdmin: boolean;
  registered: boolean;
  mustRegister: boolean;
  enforceAfter: string | null;
  enforced: boolean;
}) {
  useAuthStore.setState({
    user: {
      id: 1,
      username: 'admin',
      email: 'admin@example.com',
      firstName: 'A',
      lastName: 'Admin',
      displayName: 'Admin',
      alias: null,
      isSystemAdmin: true,
      isGuest: false,
      emailNotificationsEnabled: true,
      adminPasskey,
    },
    isAuthenticated: true,
  });
}

function renderBanner() {
  return render(
    <MemoryRouter>
      <AdminPasskeyBanner />
    </MemoryRouter>,
  );
}

describe('AdminPasskeyBanner', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('ja');
  });

  afterEach(() => {
    cleanup();
    useAuthStore.setState({ user: null, isAuthenticated: false });
  });

  it('shows the message and a link to the security settings when mustRegister is true', () => {
    setAdminPasskey({ isAdmin: true, registered: false, mustRegister: true, enforceAfter: null, enforced: false });
    renderBanner();

    expect(screen.getByTestId('admin-passkey-banner')).toBeInTheDocument();
    expect(
      screen.getByText(/管理者のアカウントには、パスキーの登録が必要になります。/),
    ).toBeInTheDocument();
    expect(screen.getByTestId('admin-passkey-banner-link')).toHaveAttribute(
      'href',
      '/settings?tab=security',
    );
  });

  it('shows the deadline when enforceAfter is set', () => {
    // ローカル時刻の正午で作る(実行環境のタイムゾーンによって日付がずれないように)
    const noon = new Date(2026, 9, 19, 12, 0, 0).toISOString();
    setAdminPasskey({ isAdmin: true, registered: false, mustRegister: true, enforceAfter: noon, enforced: false });
    renderBanner();

    expect(screen.getByText(/\(期限: 2026年10月19日\)/)).toBeInTheDocument();
  });

  it('does not show the deadline when enforceAfter is null', () => {
    setAdminPasskey({ isAdmin: true, registered: false, mustRegister: true, enforceAfter: null, enforced: false });
    renderBanner();

    expect(screen.queryByText(/期限:/)).not.toBeInTheDocument();
  });

  it('is not shown when mustRegister is false', () => {
    setAdminPasskey({ isAdmin: true, registered: true, mustRegister: false, enforceAfter: null, enforced: false });
    renderBanner();

    expect(screen.queryByTestId('admin-passkey-banner')).not.toBeInTheDocument();
  });

  it('is not shown when there is no user', () => {
    useAuthStore.setState({ user: null, isAuthenticated: false });
    renderBanner();

    expect(screen.queryByTestId('admin-passkey-banner')).not.toBeInTheDocument();
  });

  it('has no close button (it stays until the passkey is registered)', () => {
    setAdminPasskey({ isAdmin: true, registered: false, mustRegister: true, enforceAfter: null, enforced: false });
    renderBanner();

    // 案内の中にボタンは無い(リンクだけ)
    expect(screen.getByTestId('admin-passkey-banner').querySelector('button')).toBeNull();
  });
});
