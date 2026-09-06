/**
 * AiSettings.tsx — Ollama モデル設定（インスタンス全体・管理者のみ変更可）
 */

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';

interface AiSettingsData {
  ollamaModel: string;
  envDefaultModel: string;
  ollamaTimeoutSecs: number;
  envDefaultTimeoutSecs: number;
  ollamaConnected: boolean;
  availableModels: string[];
  canEdit: boolean;
}

export function AiSettings() {
  const { t } = useTranslation();
  const toast = useToast();
  const queryClient = useQueryClient();
  const [selectedModel, setSelectedModel] = useState('');
  const [selectedTimeout, setSelectedTimeout] = useState(60);

  const { data, isLoading } = useQuery<AiSettingsData>({
    queryKey: ['settings-ai'],
    queryFn: async () => (await apiClient.get('/settings/ai/')).data,
  });

  useEffect(() => {
    if (data?.ollamaModel) {
      setSelectedModel(data.ollamaModel);
    }
    if (data?.ollamaTimeoutSecs) {
      setSelectedTimeout(data.ollamaTimeoutSecs);
    }
  }, [data?.ollamaModel, data?.ollamaTimeoutSecs]);

  const saveMutation = useMutation({
    mutationFn: async (ollamaModel: string) => {
      const res = await apiClient.patch<AiSettingsData>('/settings/ai/', { ollamaModel });
      return res.data;
    },
    onSuccess: (updated) => {
      queryClient.setQueryData(['settings-ai'], updated);
      queryClient.invalidateQueries({ queryKey: ['ai-status'] });
      toast.success(t('settings.aiModelSaved'));
    },
    onError: () => {
      toast.error(t('settings.updateFailed'));
    },
  });

  const resetMutation = useMutation({
    mutationFn: async () => {
      const res = await apiClient.patch<AiSettingsData>('/settings/ai/', { ollamaModel: '' });
      return res.data;
    },
    onSuccess: (updated) => {
      queryClient.setQueryData(['settings-ai'], updated);
      queryClient.invalidateQueries({ queryKey: ['ai-status'] });
      setSelectedModel(updated.ollamaModel);
      toast.success(t('settings.aiModelReset'));
    },
    onError: () => {
      toast.error(t('settings.updateFailed'));
    },
  });

  const saveTimeoutMutation = useMutation({
    mutationFn: async (ollamaTimeoutSecs: number) => {
      const res = await apiClient.patch<AiSettingsData>('/settings/ai/', { ollamaTimeoutSecs });
      return res.data;
    },
    onSuccess: (updated) => {
      queryClient.setQueryData(['settings-ai'], updated);
      queryClient.invalidateQueries({ queryKey: ['settings-ai'] });
      setSelectedTimeout(updated.ollamaTimeoutSecs);
      toast.success('タイムアウトを保存しました');
    },
    onError: () => {
      toast.error(t('settings.updateFailed'));
    },
  });

  const resetTimeoutMutation = useMutation({
    mutationFn: async () => {
      const res = await apiClient.patch<AiSettingsData>('/settings/ai/', { resetOllamaTimeout: true });
      return res.data;
    },
    onSuccess: (updated) => {
      queryClient.setQueryData(['settings-ai'], updated);
      queryClient.invalidateQueries({ queryKey: ['settings-ai'] });
      setSelectedTimeout(updated.ollamaTimeoutSecs);
      toast.success('タイムアウトを既定値に戻しました');
    },
    onError: () => {
      toast.error(t('settings.updateFailed'));
    },
  });

  if (isLoading || !data) {
    return null;
  }

  const usingOverride = data.ollamaModel !== data.envDefaultModel;

  return (
    <section className="settings__section" data-testid="ai-settings">
      <h2 className="settings__section-title">{t('settings.aiTitle')}</h2>
      <div className="settings__card">
        <div className="settings__row">
          <div>
            <div className="settings__notification-label">{t('settings.aiOllamaStatus')}</div>
            <div className="settings__hint">
              {data.ollamaConnected ? t('ai.connected') : t('ai.disconnected')}
            </div>
          </div>
          <span>{data.ollamaConnected ? '✓' : '✗'}</span>
        </div>

        <div className="settings__row" style={{ marginTop: '1rem' }}>
          <div style={{ flex: 1 }}>
            <label className="settings__notification-label" htmlFor="ollama-model-select">
              {t('settings.aiModelLabel')}
            </label>
            <div className="settings__hint">
              {t('settings.aiEnvDefault', { model: data.envDefaultModel })}
            </div>
            {data.canEdit ? (
              <select
                id="ollama-model-select"
                className="settings__select"
                value={selectedModel}
                onChange={(e) => setSelectedModel(e.target.value)}
                disabled={!data.ollamaConnected || saveMutation.isPending}
              >
                {data.availableModels.map((model) => (
                  <option key={model} value={model}>
                    {model}
                  </option>
                ))}
              </select>
            ) : (
              <div className="settings__hint">{data.ollamaModel}</div>
            )}
          </div>
        </div>

        <div className="settings__row" style={{ marginTop: '1rem' }}>
          <div style={{ flex: 1 }}>
            <label className="settings__notification-label" htmlFor="ollama-timeout-input">
              {t('settings.aiTimeoutLabel')}
            </label>
            <div className="settings__hint">
              {t('settings.aiTimeoutHint')}
              <br />
              環境変数の既定値: {data.envDefaultTimeoutSecs}秒
            </div>
            {data.canEdit ? (
              <input
                id="ollama-timeout-input"
                className="settings__select"
                type="number"
                min="30"
                max="300"
                step="10"
                value={selectedTimeout}
                onChange={(e) => setSelectedTimeout(Number(e.target.value))}
                disabled={saveTimeoutMutation.isPending || resetTimeoutMutation.isPending}
              />
            ) : (
              <div className="settings__hint">{data.ollamaTimeoutSecs}秒</div>
            )}
          </div>
        </div>

        {data.canEdit && (
          <div className="settings__actions" style={{ marginTop: '1rem', display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
            <button
              type="button"
              className="settings__btn settings__btn--primary"
              disabled={
                !data.ollamaConnected
                || saveMutation.isPending
                || resetMutation.isPending
                || selectedModel === data.ollamaModel
              }
              onClick={() => saveMutation.mutate(selectedModel)}
            >
              {t('common.save', 'Save')} (Model)
            </button>
            <button
              type="button"
              className="settings__btn settings__btn--secondary"
              disabled={!usingOverride || resetMutation.isPending || saveMutation.isPending}
              onClick={() => resetMutation.mutate()}
            >
              {t('settings.aiResetToDefault')}
            </button>
            <button
              type="button"
              className="settings__btn settings__btn--primary"
              disabled={
                saveTimeoutMutation.isPending
                || resetTimeoutMutation.isPending
                || selectedTimeout === data.ollamaTimeoutSecs
              }
              onClick={() => saveTimeoutMutation.mutate(selectedTimeout)}
            >
              {t('common.save', 'Save')} (Timeout)
            </button>
            <button
              type="button"
              className="settings__btn settings__btn--secondary"
              disabled={
                selectedTimeout === data.envDefaultTimeoutSecs
                || resetTimeoutMutation.isPending
                || saveTimeoutMutation.isPending
              }
              onClick={() => resetTimeoutMutation.mutate()}
            >
              {t('settings.aiTimeoutReset')}
            </button>
          </div>
        )}

        {!data.canEdit && (
          <p className="settings__hint" style={{ marginTop: '0.75rem' }}>
            {t('settings.aiStaffOnly')}
          </p>
        )}
      </div>
    </section>
  );
}
