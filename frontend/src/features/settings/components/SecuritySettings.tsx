/**
 * SecuritySettings.tsx — セキュリティ設定（MFA: TOTP + Passkey/WebAuthn）
 *
 * TOTP（ワンタイムパスワード）とパスキー（WebAuthn）の登録・管理。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery } from '@tanstack/react-query';
import { startRegistration } from '@simplewebauthn/browser';
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

// Passkey/WebAuthn interfaces
interface PasskeyBeginResponse {
  creation_challenge: any;
  registration_state_json: string;
}

interface PasskeyRegisterCompleteResponse {
  ok: boolean;
  message: string;
}

interface PasskeySummary {
  id: number;
  name: string;
  created_at: string;
}

interface PasskeyListResponse {
  passkeys: PasskeySummary[];
}

export function SecuritySettings() {
  const { t } = useTranslation();
  const toast = useToast();

  // TOTP state
  const [step, setStep] = useState<'idle' | 'setup' | 'confirm'>('idle');
  const [qrCode, setQrCode] = useState<string>('');
  const [secretBase32, setSecretBase32] = useState<string>('');
  const [secretB64, setSecretB64] = useState<string>('');
  const [confirmCode, setConfirmCode] = useState<string>('');
  const [isEnabled, setIsEnabled] = useState(false);

  // Passkey state
  const [passkeyStep, setPasskeyStep] = useState<'idle' | 'registering'>('idle');

  // Passkey list query
  const { data: passkeyListData } = useQuery({
    queryKey: ['passkeys'],
    queryFn: async () => {
      const response = await apiClient.get<PasskeyListResponse>(
        '/settings/security/passkeys/'
      );
      return response.data;
    },
  });

  // TOTP 設定開始
  const beginMutation = useMutation({
    mutationFn: async () => {
      const response = await apiClient.post<BeginResponse>(
        '/settings/security/totp/begin/',
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
        '/settings/security/totp/confirm/',
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
        '/settings/security/totp/disable/',
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

  // Passkey 登録開始
  const passkeyBeginMutation = useMutation({
    mutationFn: async () => {
      const response = await apiClient.post<PasskeyBeginResponse>(
        '/settings/security/passkey/register/begin/',
        {}
      );
      return response.data;
    },
    onSuccess: (data) => {
      setPasskeyStep('registering');
      // ブラウザの WebAuthn API を呼び出す
      handlePasskeyCreate(data);
    },
    onError: (err: unknown) => {
      const detail = (err as { response?: { data?: { detail?: string } } })?.response?.data?.detail;
      toast.error(detail || t('common.error') || 'エラーが発生しました');
    },
  });

  // Passkey 登録完了
  const passkeyCompleteMutation = useMutation({
    mutationFn: async (payload: any) => {
      const response = await apiClient.post<PasskeyRegisterCompleteResponse>(
        '/settings/security/passkey/register/complete/',
        payload
      );
      return response.data;
    },
    onSuccess: () => {
      toast.success('パスキーを登録しました');
      setPasskeyStep('idle');
      // リストをリフレッシュして再取得する
      window.location.reload();
    },
    onError: (err: unknown) => {
      const detail = (err as { response?: { data?: { detail?: string } } })?.response?.data?.detail;
      toast.error(detail || t('common.error') || 'パスキー登録に失敗しました');
      setPasskeyStep('idle');
    },
  });

  // Passkey 削除
  const passkeyDeleteMutation = useMutation({
    mutationFn: async (id: number) => {
      return await apiClient.post(
        `/settings/security/passkey/${id}/delete/`,
        {}
      );
    },
    onSuccess: () => {
      toast.success('パスキーを削除しました');
      window.location.reload();
    },
    onError: (err: unknown) => {
      const detail = (err as { response?: { data?: { detail?: string } } })?.response?.data?.detail;
      toast.error(detail || t('common.error') || 'パスキー削除に失敗しました');
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

  const handlePasskeyBegin = () => {
    passkeyBeginMutation.mutate();
  };

  const handlePasskeyCreate = async (beginResponse: PasskeyBeginResponse) => {
    try {
      // webauthn-rs のCreationChallengeResponseは{ publicKey: {...} }の形。
      const optionsJSON = beginResponse.creation_challenge.publicKey ?? beginResponse.creation_challenge;
      const credential = await startRegistration({ optionsJSON });

      // startRegistrationの戻り値は既にJSON化された形式なのでそのまま送れる。
      // registration_state_jsonはReact stateではなく引数(beginResponse)から
      // 直接読むこと(setState直後に同じクロージャ内でstateを読むと、更新前の
      // 古い値になるため。実際にこれが原因で「不正な登録セッション」エラーが
      // 発生していた)。
      passkeyCompleteMutation.mutate({
        registration_state_json: beginResponse.registration_state_json,
        credential,
      });
    } catch (error: any) {
      console.error('WebAuthn registration error:', error);
      const msg = error?.message || '';
      if (/AbortError|NotAllowedError|取消|canceled|cancelled/i.test(msg)) {
        toast.error('パスキー登録がキャンセルされました');
      } else {
        toast.error(msg || 'パスキー作成に失敗しました');
      }
      setPasskeyStep('idle');
    }
  };

  const handlePasskeyDelete = (id: number) => {
    if (!window.confirm('このパスキーを削除してもよろしいですか？')) {
      return;
    }
    passkeyDeleteMutation.mutate(id);
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

        {/* パスキー（WebAuthn）セクション */}
        <div className="settings__passkey-section">
          <div className="settings__passkey-idle">
            <div className="settings__passkey-status">
              <span className="settings__passkey-label">
                🔑 パスキー（Passkey / WebAuthn）
              </span>
              <span className="settings__passkey-state">
                {(passkeyListData?.passkeys?.length ?? 0) > 0 ? (
                  <span className="settings__passkey-enabled">✓ {passkeyListData?.passkeys?.length} 登録済み</span>
                ) : (
                  <span className="settings__passkey-disabled">✗ 未登録</span>
                )}
              </span>
            </div>
            <p className="settings__passkey-desc">
              パスキーを使用して、より安全でかつ簡単にログインできます。
            </p>

            {passkeyStep === 'idle' && (
              <>
                {(passkeyListData?.passkeys?.length ?? 0) > 0 && (
                  <div className="settings__passkey-list">
                    <h4>登録済みパスキー</h4>
                    <ul className="settings__passkey-items">
                      {passkeyListData?.passkeys?.map((pk) => (
                        <li key={pk.id} className="settings__passkey-item">
                          <div className="settings__passkey-info">
                            <span className="settings__passkey-name">{pk.name}</span>
                            <span className="settings__passkey-date">{pk.created_at}</span>
                          </div>
                          <button
                            className="settings__btn settings__btn--danger settings__btn--small"
                            onClick={() => handlePasskeyDelete(pk.id)}
                            disabled={passkeyDeleteMutation.isPending}
                          >
                            削除
                          </button>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                <div className="settings__passkey-actions">
                  <button
                    className="settings__btn settings__btn--primary"
                    onClick={handlePasskeyBegin}
                    disabled={passkeyBeginMutation.isPending}
                  >
                    {passkeyBeginMutation.isPending ? '登録中...' : 'パスキーを登録'}
                  </button>
                </div>
              </>
            )}

            {passkeyStep === 'registering' && (
              <div className="settings__passkey-registering">
                <p>パスキーを設定中です...</p>
                <p>デバイスの認証（指紋認証など）を完了してください。</p>
              </div>
            )}
          </div>
        </div>
      </div>
    </section>
  );
}
