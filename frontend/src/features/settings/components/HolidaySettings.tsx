/**
 * HolidaySettings.tsx — 休日（祝日）管理
 *
 * 休日一覧表示、年指定での一括追加、削除機能
 */

import { useState, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useTranslation } from 'react-i18next';
import { useToastStore } from '@/shared/stores/toastStore';

// ─── 型定義 ─────────────────────────────────────
interface Holiday {
  id: number;
  date: string;
  name: string;
  recurring: boolean;
}

interface BulkAddResponse {
  added: number;
}

// ─── コンポーネント ──────────────────────────────

export function HolidaySettings() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { addToast } = useToastStore();
  const [deleteConfirm, setDeleteConfirm] = useState<Holiday | null>(null);
  const [bulkAddYear, setBulkAddYear] = useState<number>(new Date().getFullYear());

  // --- データ取得 ---
  const { data: holidays = [], isLoading } = useQuery<Holiday[]>({
    queryKey: ['holidays'],
    queryFn: async () => {
      const res = await apiClient.get<Holiday[]>('/holidays/');
      return res.data;
    },
  });

  // --- 一括追加 ---
  const bulkAddMutation = useMutation({
    mutationFn: (year: number) =>
      apiClient.post<BulkAddResponse>('/holidays/bulk-add/', { year }),
    onSuccess: (res) => {
      void queryClient.invalidateQueries({ queryKey: ['holidays'] });
      const added = res.data.added;
      addToast({
        message: `${added}件の祝日を追加しました`,
        type: 'success',
      });
    },
    onError: () => {
      addToast({
        message: '祝日の追加に失敗しました',
        type: 'error',
      });
    },
  });

  // --- 削除 ---
  const deleteMutation = useMutation({
    mutationFn: (id: number) => apiClient.delete(`/holidays/${id}/`),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['holidays'] });
      setDeleteConfirm(null);
      addToast({
        message: '削除しました',
        type: 'success',
      });
    },
    onError: () => {
      addToast({
        message: t('common.deleteFailed'),
        type: 'error',
      });
    },
  });

  const handleBulkAdd = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    bulkAddMutation.mutate(bulkAddYear);
  }, [bulkAddYear, bulkAddMutation]);

  // 日付でソート（昇順）
  const sortedHolidays = [...holidays].sort((a, b) =>
    new Date(a.date).getTime() - new Date(b.date).getTime()
  );

  if (isLoading) {
    return <div className="settings-empty"><div className="settings-empty__text">{t('common.loading')}</div></div>;
  }

  return (
    <div>
      {/* ヘッダー：一括追加セクション */}
      <div className="settings-section__header">
        <h2 className="settings-section__title">
          {t('settings.holidays', '休日')}
          <span className="settings-section__count">({sortedHolidays.length})</span>
        </h2>
      </div>

      {/* 一括追加フォーム */}
      <div style={{ marginBottom: 'var(--space-6)', padding: 'var(--space-4)', background: 'var(--color-bg-elevated)', borderRadius: 'var(--radius-md)', border: '1px solid var(--color-border-default)' }}>
        <div className="settings-section__header" style={{ marginBottom: 'var(--space-4)' }}>
          <h3 style={{ margin: 0, fontSize: 'var(--font-size-sm)', fontWeight: 600 }}>
            {t('settings.holidaysBulkAdd', '年ごとの祝日一括追加')}
          </h3>
        </div>
        <form onSubmit={handleBulkAdd} style={{ display: 'flex', gap: 'var(--space-2)', alignItems: 'flex-end' }}>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-1)' }}>
            <label style={{ fontSize: 'var(--font-size-sm)', fontWeight: 500, color: 'var(--color-text-secondary)' }}>
              {t('settings.year', '年')}
            </label>
            <input
              type="number"
              min="2000"
              max="2099"
              value={bulkAddYear}
              onChange={(e) => setBulkAddYear(Number(e.target.value))}
              style={{
                padding: 'var(--space-2) var(--space-3)',
                background: 'var(--color-bg-primary)',
                border: '1px solid var(--color-border-default)',
                borderRadius: 'var(--radius-md)',
                color: 'var(--color-text-primary)',
                fontSize: 'var(--font-size-sm)',
                width: 120,
              }}
            />
          </div>
          <button
            type="submit"
            disabled={bulkAddMutation.isPending}
            style={{
              padding: 'var(--space-2) var(--space-4)',
              background: 'var(--color-accent-primary)',
              color: 'var(--color-text-inverse)',
              border: 'none',
              borderRadius: 'var(--radius-md)',
              fontSize: 'var(--font-size-sm)',
              fontWeight: 600,
              cursor: bulkAddMutation.isPending ? 'not-allowed' : 'pointer',
              opacity: bulkAddMutation.isPending ? 0.6 : 1,
            }}
          >
            {bulkAddMutation.isPending ? t('common.loading', '追加中...') : t('settings.holidaysAdd', '追加')}
          </button>
        </form>
      </div>

      {/* 祝日一覧テーブル */}
      {sortedHolidays.length > 0 ? (
        <div style={{ overflowX: 'auto' }}>
          <table style={{
            width: '100%',
            borderCollapse: 'collapse',
            fontSize: 'var(--font-size-sm)',
          }}>
            <thead>
              <tr style={{ borderBottom: '1px solid var(--color-border-default)' }}>
                <th style={{ textAlign: 'left', padding: 'var(--space-3)', fontWeight: 600, color: 'var(--color-text-secondary)' }}>
                  {t('settings.date', '日付')}
                </th>
                <th style={{ textAlign: 'left', padding: 'var(--space-3)', fontWeight: 600, color: 'var(--color-text-secondary)' }}>
                  {t('settings.holidayName', '祝日名')}
                </th>
                <th style={{ textAlign: 'right', padding: 'var(--space-3)', fontWeight: 600, color: 'var(--color-text-secondary)', width: 60 }}>
                  {t('common.action', 'アクション')}
                </th>
              </tr>
            </thead>
            <tbody>
              {sortedHolidays.map((holiday) => (
                <tr key={holiday.id} style={{ borderBottom: '1px solid var(--color-border-default)' }}>
                  <td style={{ padding: 'var(--space-3)', color: 'var(--color-text-primary)' }}>
                    {new Date(holiday.date).toLocaleDateString('ja-JP')}
                  </td>
                  <td style={{ padding: 'var(--space-3)', color: 'var(--color-text-primary)' }}>
                    {holiday.name}
                  </td>
                  <td style={{ padding: 'var(--space-3)', textAlign: 'right' }}>
                    <button
                      onClick={() => setDeleteConfirm(holiday)}
                      style={{
                        background: 'none',
                        border: 'none',
                        color: 'var(--color-text-tertiary)',
                        cursor: 'pointer',
                        fontSize: '1.2em',
                        padding: 'var(--space-1)',
                      }}
                      title={t('common.delete', '削除')}
                    >
                      🗑
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="settings-empty">
          <div className="settings-empty__icon">🗓️</div>
          <div className="settings-empty__text">
            {t('settings.noHolidays', '祝日がまだありません。「年ごとの祝日一括追加」で作成してください。')}
          </div>
        </div>
      )}

      {/* 削除確認ダイアログ */}
      {deleteConfirm && (
        <div className="settings-modal__overlay" onClick={() => setDeleteConfirm(null)}>
          <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
            <div className="settings-modal__header">
              <h3 className="settings-modal__title">
                {t('settings.deleteHoliday', '祝日を削除')}
              </h3>
              <button className="settings-modal__close" onClick={() => setDeleteConfirm(null)}>×</button>
            </div>
            <div className="confirm-dialog__message">
              「<strong>{new Date(deleteConfirm.date).toLocaleDateString('ja-JP')}</strong> {deleteConfirm.name}」を削除しますか？
            </div>
            <div className="settings-form__actions">
              <button
                className="settings-form__btn settings-form__btn--secondary"
                onClick={() => setDeleteConfirm(null)}
              >
                {t('common.cancel', 'キャンセル')}
              </button>
              <button
                className="settings-form__btn settings-form__btn--danger"
                onClick={() => deleteMutation.mutate(deleteConfirm.id)}
                disabled={deleteMutation.isPending}
              >
                {deleteMutation.isPending ? t('common.loading', '削除中...') : t('common.delete', '削除')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
