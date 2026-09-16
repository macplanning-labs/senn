/**
 * SystemMailSettings.tsx — 全体メール設定（/admin）
 */

import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import '../components/AdminPage.css';
import '@/features/settings/components/SettingsPage.css';

interface SystemAdminSettings {
  mailMode: 'gmail_api' | 'smtp';
  mailSender: string;
  smtpHost: string;
  smtpPort: number;
  smtpEncryption: 'none' | 'starttls' | 'tls';
  smtpPasswordConfigured: boolean;
  gmailEnvConfigured: boolean;
  planType: string;
}

export function SystemMailSettings() {
  const toast = useToast();
  const queryClient = useQueryClient();
  const [mailMode, setMailMode] = useState<'gmail_api' | 'smtp'>('gmail_api');
  const [mailSender, setMailSender] = useState('');
  const [smtpHost, setSmtpHost] = useState('');
  const [smtpPort, setSmtpPort] = useState(587);
  const [smtpEncryption, setSmtpEncryption] = useState<'none' | 'starttls' | 'tls'>('starttls');
  const [smtpPassword, setSmtpPassword] = useState('');

  const { data, isLoading } = useQuery<SystemAdminSettings>({
    queryKey: ['system-admin-settings'],
    queryFn: async () => (await apiClient.get('/system-admin/settings/')).data,
  });

  useEffect(() => {
    if (!data) return;
    setMailMode(data.mailMode);
    setMailSender(data.mailSender);
    setSmtpHost(data.smtpHost);
    setSmtpPort(data.smtpPort);
    setSmtpEncryption(data.smtpEncryption);
    setSmtpPassword('');
  }, [data]);

  const saveMutation = useMutation({
    mutationFn: async () => {
      const payload: Record<string, unknown> = {
        mailMode,
        mailSender,
      };
      if (mailMode === 'smtp') {
        payload.smtpHost = smtpHost;
        payload.smtpPort = smtpPort;
        payload.smtpEncryption = smtpEncryption;
        if (smtpPassword.trim()) {
          payload.smtpPassword = smtpPassword;
        }
      }
      const res = await apiClient.put<SystemAdminSettings>('/system-admin/settings/', payload);
      return res.data;
    },
    onSuccess: (updated) => {
      queryClient.setQueryData(['system-admin-settings'], updated);
      setSmtpPassword('');
      toast.success('システム設定を保存しました');
    },
    onError: () => {
      toast.error('保存に失敗しました');
    },
  });

  if (isLoading || !data) {
    return null;
  }

  const smtpMode = mailMode === 'smtp';

  return (
    <section className="settings__section" data-testid="admin-mail-settings">
      <h2 className="settings__section-title">全体システム設定</h2>
      <div className="settings__card">
        <div className="admin__field">
          <label className="admin__label" htmlFor="mail-mode">メール送信モード</label>
          <select
            id="mail-mode"
            className="admin__select"
            value={mailMode}
            onChange={(e) => setMailMode(e.target.value as 'gmail_api' | 'smtp')}
          >
            <option value="gmail_api">Gmail API（サービスアカウント / .env）</option>
            <option value="smtp">汎用 SMTP</option>
          </select>
          {mailMode === 'gmail_api' && (
            <p className="admin__hint">
              Gmail API は <code>GOOGLE_SERVICE_ACCOUNT_KEY_PATH</code> と
              {' '}<code>EMAIL_HOST_USER</code> の .env 設定を継続利用します。
              {' '}
              <span className={`admin__status-badge ${data.gmailEnvConfigured ? 'admin__status-badge--ok' : 'admin__status-badge--warn'}`}>
                {data.gmailEnvConfigured ? 'env 設定済み' : 'env 未設定（DRY-RUN）'}
              </span>
            </p>
          )}
        </div>

        <div className="admin__field">
          <label className="admin__label" htmlFor="mail-sender">送信元アドレス</label>
          <input
            id="mail-sender"
            className="admin__input"
            type="email"
            value={mailSender}
            onChange={(e) => setMailSender(e.target.value)}
            required
          />
        </div>

        {smtpMode && (
          <>
            <div className="admin__field">
              <label className="admin__label" htmlFor="smtp-host">SMTP ホスト</label>
              <input
                id="smtp-host"
                className="admin__input"
                value={smtpHost}
                onChange={(e) => setSmtpHost(e.target.value)}
              />
            </div>
            <div className="admin__field">
              <label className="admin__label" htmlFor="smtp-port">SMTP ポート</label>
              <input
                id="smtp-port"
                className="admin__input"
                type="number"
                min={1}
                max={65535}
                value={smtpPort}
                onChange={(e) => setSmtpPort(Number(e.target.value))}
              />
            </div>
            <div className="admin__field">
              <label className="admin__label" htmlFor="smtp-encryption">暗号化</label>
              <select
                id="smtp-encryption"
                className="admin__select"
                value={smtpEncryption}
                onChange={(e) => setSmtpEncryption(e.target.value as 'none' | 'starttls' | 'tls')}
              >
                <option value="starttls">STARTTLS (587)</option>
                <option value="tls">TLS / SMTPS (465)</option>
                <option value="none">なし</option>
              </select>
            </div>
            <div className="admin__field">
              <label className="admin__label" htmlFor="smtp-password">SMTP パスワード</label>
              <input
                id="smtp-password"
                className="admin__input"
                type="password"
                value={smtpPassword}
                onChange={(e) => setSmtpPassword(e.target.value)}
                placeholder={data.smtpPasswordConfigured ? '変更する場合のみ入力' : 'パスワードを入力'}
                autoComplete="new-password"
              />
              {data.smtpPasswordConfigured && (
                <p className="admin__hint">パスワードは暗号化して保存済みです。空欄のまま保存すると既存値を維持します。</p>
              )}
            </div>
          </>
        )}

        <div className="admin__actions">
          <button
            type="button"
            className="admin__btn admin__btn--primary"
            disabled={saveMutation.isPending}
            onClick={() => saveMutation.mutate()}
          >
            保存
          </button>
        </div>
      </div>
    </section>
  );
}
