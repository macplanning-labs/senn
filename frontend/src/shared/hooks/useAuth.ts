import { useAuthStore } from '@/shared/stores/authStore';

/**
 * 現在のユーザー情報を取得するフック
 *
 * useAuthStore の user をシンプルに取得
 */
export function useAuth() {
  const { user } = useAuthStore();
  return { user };
}
