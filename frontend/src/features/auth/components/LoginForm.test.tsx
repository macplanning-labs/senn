// @vitest-environment jsdom
/**
 * LoginForm.test.tsx — 管理者のパスキー必須(A-4b)のログイン画面
 *
 * - パスキーの確認の段階(passkey_required): begin → WebAuthn → verify → ログイン
 * - パスキーの新規登録の段階(passkey_enroll_required): begin → WebAuthn の登録 → complete → ログイン
 * - 403 passkey_enroll_not_allowed は、サーバーの説明をそのまま表示する
 * - 通常のパスワードのログインと、TOTP の流れは変わらない
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { startAuthentication, startRegistration } from '@simplewebauthn/browser';
import i18n from '@/i18n';
import { apiClient } from '@/shared/api/client';
import { useAuthStore } from '@/shared/stores/authStore';
import { LoginForm } from './LoginForm';

vi.mock('@/shared/api/client');
vi.mock('@simplewebauthn/browser', () => ({
  startAuthentication: vi.fn(),
  startRegistration: vi.fn(),
}));
vi.mock('@/shared/sync/pull', () => ({ clearCachedQueries: vi.fn() }));
vi.mock('@/shared/api/mediaSession', () => ({
  ensureMediaSession: vi.fn(async () => {}),
  ensureMediaSessionWithin: vi.fn(async () => {}),
  startMediaSessionKeepAlive: vi.fn(),
  stopMediaSessionKeepAlive: vi.fn(),
}));

const mockPost = apiClient.post as unknown as ReturnType<typeof vi.fn>;
const mockGet = apiClient.get as unknown as ReturnType<typeof vi.fn>;
const mockStartAuthentication = vi.mocked(startAuthentication);
const mockStartRegistration = vi.mocked(startRegistration);

const originalLoginWithPasskey = useAuthStore.getState().loginWithPasskey;

/** axios のエラーの形(isAxiosError が true になる形) */
function axiosError(status: number, data: unknown) {
  return Object.assign(new Error('Request failed'), {
    isAxiosError: true,
    response: { status, data },
  });
}

/** URL ごとの応答を決める。決めていない URL への送信は失敗させる(想定外の呼び出しを見つけるため) */
function postRoutes(routes: Record<string, unknown>) {
  mockPost.mockImplementation(async (url: string) => {
    if (!(url in routes)) throw new Error(`unexpected POST ${url}`);
    const response = routes[url];
    if (response instanceof Error) throw response;
    return { data: response };
  });
}

function postedUrls(): string[] {
  return mockPost.mock.calls.map((call) => call[0] as string);
}

function renderLogin() {
  return render(
    <MemoryRouter initialEntries={['/']}>
      <Routes>
        <Route path="/" element={<LoginForm />} />
        <Route path="/my-issues" element={<div>MY ISSUES</div>} />
      </Routes>
    </MemoryRouter>,
  );
}

async function submitPassword(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByTestId('login-username'), 'admin');
  await user.type(screen.getByTestId('login-password'), 'pw-for-test');
  await user.click(screen.getByTestId('login-submit'));
}

describe('LoginForm: 管理者のパスキー(A-4b)', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('ja');
    // 自動の SSO 試行は、この試験では動かさない
    sessionStorage.setItem('senn_sso_skip_auto', '1');
    mockGet.mockResolvedValue({ data: { id: 1, username: 'admin' } });
    mockStartAuthentication.mockReset();
    mockStartRegistration.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    mockPost.mockReset();
    sessionStorage.clear();
    useAuthStore.setState({ loginWithPasskey: originalLoginWithPasskey });
    useAuthStore.setState({ user: null, isAuthenticated: false });
  });

  it('passkey_required: shows the passkey step, verifies with begin → WebAuthn → verify, then signs in', async () => {
    const user = userEvent.setup();
    postRoutes({
      '/auth/login/': { passkey_required: true, passkey_token: 'PT' },
      '/auth/login/passkey/begin/': {
        request_challenge: { publicKey: { challenge: 'C' } },
        auth_token: 'AT',
      },
      '/auth/login/passkey/verify/': { access: 'A', refresh: 'R' },
    });
    mockStartAuthentication.mockResolvedValue({ id: 'cred' } as any);
    const loginWithPasskey = vi.fn(async () => {});
    useAuthStore.setState({ loginWithPasskey });

    renderLogin();
    await submitPassword(user);

    expect(await screen.findByTestId('passkey-step-form')).toBeInTheDocument();
    expect(screen.getByText('管理者のアカウントは、パスキーでの確認が必要です。')).toBeInTheDocument();
    expect(screen.queryByTestId('mfa-code-input')).not.toBeInTheDocument();

    await user.click(screen.getByTestId('passkey-step-submit'));

    await waitFor(() => expect(loginWithPasskey).toHaveBeenCalledWith('A', 'R'));
    expect(postedUrls()).toEqual([
      '/auth/login/',
      '/auth/login/passkey/begin/',
      '/auth/login/passkey/verify/',
    ]);
    expect(mockPost.mock.calls[1]?.[1]).toEqual({ passkey_token: 'PT' });
    expect(mockStartAuthentication).toHaveBeenCalledWith({ optionsJSON: { challenge: 'C' } });
    expect(mockPost.mock.calls[2]?.[1]).toEqual({
      passkey_token: 'PT',
      auth_token: 'AT',
      credential: { id: 'cred' },
    });
    expect(await screen.findByText('MY ISSUES')).toBeInTheDocument();
  });

  it('passkey_enroll_required: shows the enroll step, registers with begin → WebAuthn → complete, then signs in', async () => {
    const user = userEvent.setup();
    postRoutes({
      '/auth/login/': { passkey_enroll_required: true, enroll_token: 'ET' },
      '/auth/login/passkey-enroll/begin/': {
        creation_challenge: { publicKey: { challenge: 'K' } },
        registration_token: 'RT',
      },
      '/auth/login/passkey-enroll/complete/': { access: 'A', refresh: 'R' },
    });
    mockStartRegistration.mockResolvedValue({ id: 'new' } as any);
    const loginWithPasskey = vi.fn(async () => {});
    useAuthStore.setState({ loginWithPasskey });

    renderLogin();
    await submitPassword(user);

    expect(await screen.findByTestId('passkey-enroll-form')).toBeInTheDocument();
    expect(
      screen.getByText('このアカウントのパスキーはリセットされています。新しいパスキーを登録してください。'),
    ).toBeInTheDocument();

    await user.click(screen.getByTestId('passkey-enroll-submit'));

    await waitFor(() => expect(loginWithPasskey).toHaveBeenCalledWith('A', 'R'));
    expect(postedUrls()).toEqual([
      '/auth/login/',
      '/auth/login/passkey-enroll/begin/',
      '/auth/login/passkey-enroll/complete/',
    ]);
    expect(mockPost.mock.calls[1]?.[1]).toEqual({ enroll_token: 'ET' });
    expect(mockStartRegistration).toHaveBeenCalledWith({ optionsJSON: { challenge: 'K' } });
    expect(mockPost.mock.calls[2]?.[1]).toEqual({
      enroll_token: 'ET',
      registration_token: 'RT',
      credential: { id: 'new' },
    });
  });

  it('403 passkey_enroll_not_allowed: shows the server explanation and no enroll step', async () => {
    const user = userEvent.setup();
    postRoutes({
      '/auth/login/': axiosError(403, {
        detail: 'テスト用の説明: 別の管理者に依頼してください',
        code: 'passkey_enroll_not_allowed',
      }),
    });

    renderLogin();
    await submitPassword(user);

    expect(await screen.findByTestId('login-error')).toHaveTextContent(
      'テスト用の説明: 別の管理者に依頼してください',
    );
    expect(screen.queryByTestId('passkey-enroll-form')).not.toBeInTheDocument();
    expect(screen.queryByTestId('passkey-step-form')).not.toBeInTheDocument();
  });

  it('passkey step: "やり直す" returns to the password form', async () => {
    const user = userEvent.setup();
    postRoutes({ '/auth/login/': { passkey_required: true, passkey_token: 'PT' } });

    renderLogin();
    await submitPassword(user);
    expect(await screen.findByTestId('passkey-step-form')).toBeInTheDocument();

    await user.click(screen.getByTestId('passkey-step-back'));

    expect(screen.queryByTestId('passkey-step-form')).not.toBeInTheDocument();
    expect(screen.getByTestId('login-form')).toBeInTheDocument();
  });

  it('password only: signs in directly without showing a passkey step', async () => {
    const user = userEvent.setup();
    postRoutes({ '/auth/login/': { access: 'A', refresh: 'R' } });

    renderLogin();
    await submitPassword(user);

    expect(await screen.findByText('MY ISSUES')).toBeInTheDocument();
    expect(mockPost).toHaveBeenCalledTimes(1);
  });

  it('mfa_required (TOTP): still shows the authenticator code step, not the passkey step', async () => {
    const user = userEvent.setup();
    postRoutes({ '/auth/login/': { mfa_required: true, mfa_token: 'MT' } });

    renderLogin();
    await submitPassword(user);

    expect(await screen.findByTestId('mfa-code-input')).toBeInTheDocument();
    expect(screen.queryByTestId('passkey-step-form')).not.toBeInTheDocument();
    expect(screen.queryByTestId('passkey-enroll-form')).not.toBeInTheDocument();
  });
});
