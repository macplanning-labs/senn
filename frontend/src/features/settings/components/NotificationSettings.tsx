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

function getEventCategories(t: (key: string) => string): { key: string; icon: string; label: string }[] {
  return [
    { key: 'assigned', icon: '👤', label: t('notifications.eventAssigned') },
    { key: 'commented', icon: '💬', label: t('notifications.eventCommented') },
    { key: 'status_changed', icon: '🔄', label: t('notifications.eventStatusChanged') },
    { key: 'updated', icon: '📝', label: t('notifications.eventUpdated') },
    { key: 'mentioned', icon: '📢', label: t('notifications.eventMentioned') },
    { key: 'review_requested', icon: '👁️', label: t('notifications.eventReviewRequested') },
    { key: 'due_soon', icon: '⏰', label: t('notifications.eventDueSoon') },
    { key: 'overdue', icon: '🔥', label: t('notifications.eventOverdue') },
  ];
}

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
            {getEventCategories(t).map((cat) => {
              const pref = preferences.find((p) => p.category === cat.key);
              const checked = pref?.emailEnabled ?? true;
              return (
                <div key={cat.key} className="settings__notification-row">
                  <div>
                    <div className="settings__notification-label">
                      {cat.icon} {cat.label}
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
