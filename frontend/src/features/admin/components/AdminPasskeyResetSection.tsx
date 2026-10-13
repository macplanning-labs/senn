/**
 * AdminPasskeyResetSection.tsx — 管理者によるパスキーのリセット(DEMO-000153 A-4a)
 *
 * パスキーを失くした人のパスキーを、別のシステム管理者が削除する。
 * - 自分自身の行には出さない(パスワードだけを知る攻撃者が、自分のパスキーを入れ替えられないようにするため)
 * - 押すと確認ダイアログを出し、「実行する」を押したときだけ API を呼ぶ
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useAuthStore } from '@/shared/stores/authStore';
import { useToast } from '@/shared/stores/toastStore';
import { ConfirmDialog } from '@/shared/components/ui/ConfirmDialog';

interface UserListItem {
  id: number;
  username: string;
  displayName: string;
}

interface ResetResult {
  deleted: number;
  notified: boolean;
  /** リセットした本人が、パスワードで登録し直せる時間(時間。A-4b) */
  enrollWindowHours: number;
}

export function AdminPasskeyResetSection() {
  const { t } = useTranslation();
  const toast = useToast();
  const queryClient = useQueryClient();
  const selfId = useAuthStore((s) => s.user?.id);
  const [target, setTarget] = useState<UserListItem | null>(null);

  const { data: users, isLoading } = useQuery({
    queryKey: ['users'],
    queryFn: async () => (await apiClient.get<UserListItem[]>('/users/')).data,
  });

  const reset = useMutation({
    mutationFn: async (userId: number) => {
      const res = await apiClient.post<ResetResult>(`/system-admin/users/${userId}/passkeys/reset/`);
      return res.data;
    },
    onSuccess: (data) => {
      toast.success(t('adminPasskey.resetDone', { count: data.deleted }));
      void queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: () => {
      toast.error(t('adminPasskey.resetFailed'));
    },
    onSettled: () => {
      setTarget(null);
    },
  });

  const others = (users ?? []).filter((u) => u.id !== selfId);

  return (
    <section className="settings__section" data-testid="admin-passkey-reset">
      <h2 className="settings__section-title">{t('adminPasskey.resetHeading')}</h2>
      <div className="settings__card">
        <p className="admin__hint" style={{ marginBottom: '1rem' }}>{t('adminPasskey.resetDescription')}</p>
        {isLoading ? (
          <p className="admin__hint">{t('common.loading')}</p>
        ) : (
          <ul className="admin-access__list">
            {others.map((u) => (
              <li key={u.id} className="admin-access__row" data-testid={`passkey-reset-row-${u.id}`}>
                <span className="admin-access__name">{u.displayName || u.username}</span>
                <span className="admin-access__meta">{u.username}</span>
                <button
                  type="button"
                  className="admin__btn admin__btn--danger"
                  disabled={reset.isPending}
                  onClick={() => setTarget(u)}
                  data-testid={`passkey-reset-${u.id}`}
                >
                  {t('adminPasskey.resetButton')}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <ConfirmDialog
        open={target !== null}
        title={t('adminPasskey.resetConfirmTitle')}
        message={t('adminPasskey.resetConfirmBody')}
        confirmLabel={t('adminPasskey.resetConfirmOk')}
        cancelLabel={t('adminPasskey.cancel')}
        danger
        busy={reset.isPending}
        onConfirm={() => {
          if (target) reset.mutate(target.id);
        }}
        onCancel={() => setTarget(null)}
      />
    </section>
  );
}
