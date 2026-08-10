/**
 * SecuritySettings.tsx — セキュリティ設定（TOTP多要素認証）
 *
 * TOTP（ワンタイムパスワード）の登録・無効化。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';

interface BeginResponse {
  secret_b64: string;
  secret_base32: string;
  qr_base64: string;
}

interface ConfirmResponse {
  ok: boolean;
}

interface DisableResponse {
  ok: boolean;
}

export function SecuritySettings() {
  const { t } = useTranslation();
  const toast = useToast();

  const [step, setStep] = useState<'idle' | 'setup' | 'confirm'>('idle');
  const [qrCode, setQrCode] = useState<string>('');
  const [secretBase32, setSecretBase32] = useState<string>('');
  const [secretB64, setSecretB64] = useState<string>('');
  const [confirmCode, setConfirmCode] = useState<string>('');
  const [isEnabled, setIsEnabled] = useState(false);

  // TOTP 設定開始
  const beginMutation = useMutation({
    mutationFn: async () => {
      const response = await apiClient.post<BeginResponse>(
        '/api/v1/settings/security/totp/begin/',
        {}
      );
      return response;
    },
    onSuccess: (response) => {
      const data = response.data;
      setQrCode(data.qr_base64);
      setSecretBase32(data.secret_base32);
      setSecretB64(data.secret_b64);
      setStep('setup');
    },
    onError: () => {
      toast.error(t('common.error') || 'エラーが発生しました');
    },
  });

  // TOTP 設定確認
  const confirmMutation = useMutation({
    mutationFn: async () => {
      const response = await apiClient.post<ConfirmResponse>(
        '/api/v1/settings/security/totp/confirm/',
        {
          secret_b64: secretB64,
          code: confirmCode,
        }
      );
      return response;
    },
    onSuccess: () => {
      toast.success(t('settings.totpEnabled') || 'TOTPが有効になりました');
      setIsEnabled(true);
      setStep('idle');
      setConfirmCode('');
      setQrCode('');
    },
    onError: () => {
      toast.error(t('settings.totpConfirmFailed') || '確認に失敗しました');
    },
  });

  // TOTP 無効化
  const disableMutation = useMutation({
    mutationFn: async (password: string) => {
      const response = await apiClient.post<DisableResponse>(
        '/api/v1/settings/security/totp/disable/',
        { password }
      );
      return response;
    },
    onSuccess: () => {
      toast.success(t('settings.totpDisabled') || 'TOTPが無効になりました');
      setIsEnabled(false);
      setStep('idle');
    },
    onError: (err: unknown) => {
      const detail = (err as { response?: { data?: { detail?: string } } })?.response?.data?.detail;
      toast.error(detail || t('common.error') || 'エラーが発生しました');
    },
  });

  const handleBegin = () => {
    beginMutation.mutate();
  };

  const handleConfirm = () => {
    if (confirmCode.length !== 6) {
      toast.error(t('settings.totpCodeLength') || '6桁のコードを入力してください');
      return;
    }
    confirmMutation.mutate();
  };

  const handleDisable = () => {
    if (!window.confirm(t('settings.totpDisableConfirm') || 'TOTPを無効化してもよろしいですか？')) {
      return;
    }
    const password = window.prompt(t('settings.totpDisablePasswordPrompt') || '確認のため、現在のパスワードを入力してください');
    if (!password) {
      return;
    }
    disableMutation.mutate(password);
  };

  return (
    <section className="settings__section">
      <h2 className="settings__section-title">🔐 {t('settings.security', 'Security')}</h2>
      <div className="settings__card">
        {step === 'idle' && (
          <div className="settings__totp-idle">
            <div className="settings__totp-status">
              <span className="settings__totp-label">
                {t('settings.totpMultiFactor', 'Two-Factor Authentication (TOTP)')}
              </span>
              <span className="settings__totp-state">
                {isEnabled ? (
                  <span className="settings__totp-enabled">✓ {t('common.enabled', 'Enabled')}</span>
                ) : (
                  <span className="settings__totp-disabled">✗ {t('common.disabled', 'Disabled')}</span>
                )}
              </span>
            </div>
            <p className="settings__totp-desc">
              {t('settings.totpDesc', 'Use an authenticator app like Google Authenticator to add an extra layer of security.')}
            </p>
            <div className="settings__totp-actions">
              {!isEnabled ? (
                <button
                  className="settings__btn settings__btn--primary"
                  onClick={handleBegin}
                  disabled={beginMutation.isPending}
                >
                  {t('settings.totpEnable', 'Enable TOTP')}
                </button>
              ) : (
                <button
                  className="settings__btn settings__btn--danger"
                  onClick={handleDisable}
                  disabled={disableMutation.isPending}
                >
                  {t('settings.totpDisable', 'Disable TOTP')}
                </button>
              )}
            </div>
          </div>
        )}

        {step === 'setup' && (
          <div className="settings__totp-setup">
            <h3 className="settings__totp-setup-title">
              {t('settings.totpSetup', 'Set Up Two-Factor Authentication')}
            </h3>
            <p className="settings__totp-setup-desc">
              {t('settings.totpSetupDesc', 'Scan this QR code with your authenticator app:')}
            </p>

            {qrCode && (
              <div className="settings__totp-qr">
                <img
                  src={`data:image/png;base64,${qrCode}`}
                  alt="TOTP QR Code"
                  className="settings__qr-image"
                />
              </div>
            )}

            <div className="settings__totp-manual">
              <p className="settings__manual-label">
                {t('settings.totpManualEntry', 'Or enter this code manually:')}
              </p>
              <code className="settings__manual-code">{secretBase32}</code>
              <button
                className="settings__copy-btn"
                onClick={() => {
                  navigator.clipboard.writeText(secretBase32);
                  toast.success(t('common.copied', 'Copied to clipboard'));
                }}
              >
                📋 {t('common.copy', 'Copy')}
              </button>
            </div>

            <div className="settings__totp-verify">
              <label className="settings__totp-input-label">
                {t('settings.totpEnter6Digit', 'Enter the 6-digit code from your app:')}
              </label>
              <input
                type="text"
                inputMode="numeric"
                maxLength={6}
                pattern="[0-9]{6}"
                value={confirmCode}
                onChange={(e) => setConfirmCode(e.target.value.replace(/[^0-9]/g, '').slice(0, 6))}
                placeholder="000000"
                className="settings__totp-input"
              />
            </div>

            <div className="settings__totp-actions">
              <button
                className="settings__btn settings__btn--secondary"
                onClick={() => {
                  setStep('idle');
                  setConfirmCode('');
                  setQrCode('');
                }}
                disabled={confirmMutation.isPending}
              >
                {t('common.cancel', 'Cancel')}
              </button>
              <button
                className="settings__btn settings__btn--primary"
                onClick={handleConfirm}
                disabled={confirmMutation.isPending || confirmCode.length !== 6}
              >
                {confirmMutation.isPending ? t('common.loading', 'Loading...') : t('common.confirm', 'Confirm')}
              </button>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
