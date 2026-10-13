// @vitest-environment jsdom
/**
 * AdminPasskeyResetSection.test.tsx — 管理者によるパスキーのリセット
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import i18n from '@/i18n';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import { useAuthStore } from '@/shared/stores/authStore';
import { AdminPasskeyResetSection } from './AdminPasskeyResetSection';

vi.mock('@/shared/api/client');
vi.mock('@/shared/stores/toastStore');

const mockApiClient = apiClient as any;
const mockUseToast = useToast as any;

const USERS = [
  { id: 1, username: 'me', displayName: '自分', firstName: '', lastName: '' },
  { id: 2, username: 'taro', displayName: '太郎', firstName: '', lastName: '' },
];

let toast: { success: ReturnType<typeof vi.fn>; error: ReturnType<typeof vi.fn> };

function renderSection() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <AdminPasskeyResetSection />
    </QueryClientProvider>,
  );
}

describe('AdminPasskeyResetSection', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('ja');
    toast = { success: vi.fn(), error: vi.fn() };
    mockUseToast.mockReturnValue(toast);
    mockApiClient.get.mockResolvedValue({ data: USERS });
    mockApiClient.post.mockResolvedValue({ data: { deleted: 3, notified: true } });
    // ログイン中の本人は id 1
    useAuthStore.setState({
      user: { id: 1, username: 'me' } as any,
      isAuthenticated: true,
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    useAuthStore.setState({ user: null, isAuthenticated: false });
  });

  it('does not show a reset button on the row of the signed-in user', async () => {
    renderSection();

    await waitFor(() => expect(screen.getByTestId('passkey-reset-2')).toBeInTheDocument());
    expect(screen.queryByTestId('passkey-reset-1')).not.toBeInTheDocument();
    expect(screen.queryByTestId('passkey-reset-row-1')).not.toBeInTheDocument();
  });

  it('does not call the API until the confirmation is accepted', async () => {
    const user = userEvent.setup();
    renderSection();

    await user.click(await screen.findByTestId('passkey-reset-2'));

    expect(screen.getByTestId('confirm-dialog')).toBeInTheDocument();
    expect(screen.getByText(/このユーザーのパスキーをすべて削除します。/)).toBeInTheDocument();
    expect(mockApiClient.post).not.toHaveBeenCalled();
  });

  it('does not call the API when the confirmation is cancelled', async () => {
    const user = userEvent.setup();
    renderSection();

    await user.click(await screen.findByTestId('passkey-reset-2'));
    await user.click(screen.getByTestId('confirm-dialog-cancel'));

    expect(screen.queryByTestId('confirm-dialog')).not.toBeInTheDocument();
    expect(mockApiClient.post).not.toHaveBeenCalled();
  });

  it('calls the reset API for the chosen user after confirmation and reports the count', async () => {
    const user = userEvent.setup();
    renderSection();

    await user.click(await screen.findByTestId('passkey-reset-2'));
    await user.click(screen.getByTestId('confirm-dialog-confirm'));

    await waitFor(() =>
      expect(mockApiClient.post).toHaveBeenCalledWith('/system-admin/users/2/passkeys/reset/'),
    );
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith('3 件のパスキーを削除しました'));
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('shows an error toast when the reset fails', async () => {
    mockApiClient.post.mockRejectedValue(new Error('boom'));
    const user = userEvent.setup();
    renderSection();

    await user.click(await screen.findByTestId('passkey-reset-2'));
    await user.click(screen.getByTestId('confirm-dialog-confirm'));

    await waitFor(() => expect(toast.error).toHaveBeenCalledWith(expect.any(String)));
    expect(toast.success).not.toHaveBeenCalled();
  });
});
