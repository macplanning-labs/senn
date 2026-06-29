/**
 * authStore.ts — 認証状態管理
 *
 * JWTトークンによるログイン/ログアウト状態を管理。
 * apiClient のトークン管理関数と連携。
 */

import { create } from 'zustand';
import { apiClient, setTokens, clearTokens, getAccessToken } from '@/shared/api/client';

interface User {
  id: number;
  username: string;
  email: string;
  firstName: string;
  lastName: string;
  isStaff: boolean;
}

interface AuthState {
  /** 現在のユーザー（未認証時はnull） */
  user: User | null;
  /** 認証状態の読み込み中フラグ */
  isLoading: boolean;
  /** 認証済みかどうか */
  isAuthenticated: boolean;

  // アクション
  login: (username: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  fetchUser: () => Promise<void>;
  setUser: (user: User | null) => void;
}

export const useAuthStore = create<AuthState>()((set) => ({
  user: null,
  isLoading: true,
  isAuthenticated: !!getAccessToken(),

  login: async (username, password) => {
    const { data } = await apiClient.post<{ access: string; refresh: string }>(
      '/auth/login/',
      { username, password },
    );
    setTokens(data.access, data.refresh);
    set({ isAuthenticated: true });

    // ユーザー情報を取得
    const userRes = await apiClient.get<User>('/auth/me/');
    set({ user: userRes.data, isLoading: false });
  },

  logout: async () => {
    try {
      await apiClient.post('/auth/logout/');
    } catch {
      // ログアウトAPI失敗でもローカル状態はクリア
    }
    clearTokens();
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
}));
