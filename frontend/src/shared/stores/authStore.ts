/**
 * authStore.ts — 認証状態管理
 *
 * JWTトークンによるログイン/ログアウト状態を管理。
 * apiClient のトークン管理関数と連携。
 */

import { create } from 'zustand';
import { apiClient, setTokens, clearTokens, getAccessToken, getRefreshToken } from '@/shared/api/client';
import { enableDemoMode, DEMO_ACCESS_TOKEN } from '@/features/demo/demoMode';

interface User {
  id: number;
  username: string;
  email: string;
  firstName: string;
  lastName: string;
  displayName: string;
  alias?: string | null;
  isStaff: boolean;
  emailNotificationsEnabled: boolean;
}

/** login() の結果。MFA登録済みユーザーは即ログインせずmfaTokenを返す */
export type LoginResult =
  | { mfaRequired: false }
  | { mfaRequired: true; mfaToken: string };

interface AuthState {
  /** 現在のユーザー（未認証時はnull） */
  user: User | null;
  /** 認証状態の読み込み中フラグ */
  isLoading: boolean;
  /** 認証済みかどうか */
  isAuthenticated: boolean;

  // アクション
  login: (username: string, password: string) => Promise<LoginResult>;
  verifyMfa: (mfaToken: string, totpCode: string) => Promise<void>;
  loginWithPasskey: (access: string, refresh: string) => Promise<void>;
  logout: () => Promise<void>;
  fetchUser: () => Promise<void>;
  setUser: (user: User | null) => void;
  enterDemoMode: () => Promise<void>;
}

async function completeLogin(
  set: (partial: Partial<AuthState>) => void,
  access: string,
  refresh: string,
) {
  setTokens(access, refresh);
  set({ isAuthenticated: true });
  const userRes = await apiClient.get<User>('/auth/me/');
  set({ user: userRes.data, isLoading: false });
}

export const useAuthStore = create<AuthState>()((set) => ({
  user: null,
  isLoading: true,
  isAuthenticated: !!getAccessToken(),

  login: async (username, password) => {
    const { data } = await apiClient.post<
      { access: string; refresh: string } | { mfa_required: true; mfa_token: string }
    >('/auth/login/', { username, password });

    if ('mfa_required' in data) {
      return { mfaRequired: true, mfaToken: data.mfa_token };
    }

    await completeLogin(set, data.access, data.refresh);
    return { mfaRequired: false };
  },

  verifyMfa: async (mfaToken, totpCode) => {
    const { data } = await apiClient.post<{ access: string; refresh: string }>(
      '/auth/login/verify/',
      { mfa_token: mfaToken, totp_code: totpCode },
    );
    await completeLogin(set, data.access, data.refresh);
  },

  loginWithPasskey: async (access, refresh) => {
    await completeLogin(set, access, refresh);
  },

  logout: async () => {
    try {
      // ボディを省略するとContent-Typeが付かずJson抽出器に415で弾かれるため、
      // リフレッシュトークンを明示的に送る(サーバー側のブラックリスト登録に必要)
      await apiClient.post('/auth/logout/', { refresh: getRefreshToken() });
    } catch {
      // ログアウトAPI失敗でもローカル状態はクリア
    }
    clearTokens();
    const { clearDemoMode } = await import('@/features/demo/demoMode');
    clearDemoMode();
    set({ user: null, isAuthenticated: false, isLoading: false });
  },

  fetchUser: async () => {
    const token = getAccessToken();
    if (!token) {
      set({ user: null, isAuthenticated: false, isLoading: false });
      return;
    }
    try {
      const { data } = await apiClient.get<User>('/auth/me/');
      set({ user: data, isAuthenticated: true, isLoading: false });
    } catch {
      clearTokens();
      set({ user: null, isAuthenticated: false, isLoading: false });
    }
  },

  setUser: (user) => set({ user, isAuthenticated: !!user }),

  enterDemoMode: async () => {
    enableDemoMode();
    setTokens(DEMO_ACCESS_TOKEN, DEMO_ACCESS_TOKEN);
    set({ isAuthenticated: true });
    try {
      const userRes = await apiClient.get<User>('/auth/me/');
      set({ user: userRes.data, isLoading: false });
    } catch {
      set({ isLoading: false });
    }
  },
}));
