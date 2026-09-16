/**
 * NotificationSettings.tsx — メール通知イベント別設定
 *
 * マスターON/OFF(user.emailNotificationsEnabled)と、
 * イベント種別ごとのON/OFF(/auth/me/notification-preferences/)を扱う。
 */

import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useAuthStore } from '@/shared/stores/authStore';
import { useToast } from '@/shared/stores/toastStore';

interface NotificationPreference {
  category: string;
  emailEnabled: boolean;
}

const CATEGORY_META: { key: string; icon: string; labelKey: string; fallback: string }[] = [
  { key: 'assigned', icon: '👤', labelKey: 'settings.assignedToMe', fallback: '担当設定' },
  { key: 'commented', icon: '💬', labelKey: 'settings.commentsOnTickets', fallback: 'コメント' },
  { key: 'status_changed', icon: '🔄', labelKey: 'settings.statusChanges', fallback: 'ステータス変更' },
  { key: 'updated', icon: '📝', labelKey: 'settings.ticketUpdated', fallback: '更新' },
  { key: 'mentioned', icon: '📢', labelKey: 'settings.mentioned', fallback: 'メンション' },
  { key: 'review_requested', icon: '👁️', labelKey: 'settings.reviewRequested', fallback: 'レビュー依頼' },
  { key: 'due_soon', icon: '⏰', labelKey: 'settings.dueReminders', fallback: '期限間近' },
  { key: 'overdue', icon: '🔥', labelKey: 'settings.overdue', fallback: '期限超過' },
];

export function NotificationSettings() {
  const { t } = useTranslation();
  const { user, setUser } = useAuthStore();
  const toast = useToast();
  const queryClient = useQueryClient();

  const preferencesQuery = useQuery({
    queryKey: ['notification-preferences'],
    queryFn: async () => {
      const { data } = await apiClient.get<NotificationPreference[]>('/auth/me/notification-preferences/');
      return data;
    },
  });

  const toggleEmailMutation = useMutation({
    mutationFn: async (enabled: boolean) => {
      const { data } = await apiClient.patch('/auth/me/', {
        email_notifications_enabled: enabled,
      });
      return data as { emailNotificationsEnabled: boolean };
    },
    onSuccess: (data) => {
      if (user) setUser({ ...user, emailNotificationsEnabled: data.emailNotificationsEnabled });
      toast.success(data.emailNotificationsEnabled ? t('settings.emailEnabled') : t('settings.emailDisabled'));
    },
    onError: () => {
      toast.error(t('settings.updateFailed'));
    },
  });

  const togglePreferenceMutation = useMutation({
    mutationFn: async (pref: { category: string; emailEnabled: boolean }) => {
      const { data } = await apiClient.patch<NotificationPreference[]>('/auth/me/notification-preferences/', {
        category: pref.category,
        email_enabled: pref.emailEnabled,
      });
      return data;
    },
    onSuccess: (data) => {
      queryClient.setQueryData(['notification-preferences'], data);
    },
    onError: () => {
      toast.error(t('settings.updateFailed'));
    },
  });

  const emailEnabled = user?.emailNotificationsEnabled ?? true;
  const preferences = preferencesQuery.data ?? [];

  return (
    <section className="settings__section">
      <h2 className="settings__section-title">📧 {t('settings.emailNotifications')}</h2>
      <div className="settings__card">
        <div className="settings__notification-master">
          <div className="settings__notification-row">
            <div>
              <div className="settings__notification-label">{t('settings.emailNotifications')}</div>
              <div className="settings__notification-desc">{t('settings.emailNotificationsDesc')}</div>
            </div>
            <label className="settings__toggle">
              <input
                type="checkbox"
                checked={emailEnabled}
                onChange={(e) => toggleEmailMutation.mutate(e.target.checked)}
              />
              <span className="settings__toggle-slider" />
            </label>
          </div>
        </div>

        {emailEnabled && (
          <div className="settings__notification-categories">
            {CATEGORY_META.map((cat) => {
              const pref = preferences.find((p) => p.category === cat.key);
              const checked = pref?.emailEnabled ?? true;
              return (
                <div key={cat.key} className="settings__notification-row">
                  <div>
                    <div className="settings__notification-label">
                      {cat.icon} {t(cat.labelKey, cat.fallback)}
                    </div>
                  </div>
                  <label className="settings__toggle">
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={(e) =>
                        togglePreferenceMutation.mutate({ category: cat.key, emailEnabled: e.target.checked })
                      }
                    />
                    <span className="settings__toggle-slider" />
                  </label>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}
