/**
 * AdminBackupSection.tsx — 手動 DB バックアップ（エクスポートのみ）
 */

import { useState } from 'react';
import { getAccessToken } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import '../components/AdminPage.css';
import '@/features/settings/components/SettingsPage.css';

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? '';

export function AdminBackupSection() {
  const toast = useToast();
  const [exporting, setExporting] = useState(false);

  const handleExport = async () => {
    setExporting(true);
    try {
      const token = getAccessToken();
      const res = await fetch(`${API_BASE_URL}/api/v1/system-admin/backup/export/`, {
        method: 'POST',
        headers: token ? { Authorization: `Bearer ${token}` } : {},
      });

      if (!res.ok) {
        let message = 'バックアップの取得に失敗しました';
        try {
          const body = await res.json();
          if (body?.error) message = body.error;
        } catch {
          // ignore
        }
        throw new Error(message);
      }

      const blob = await res.blob();
      const disposition = res.headers.get('Content-Disposition') ?? '';
      const match = disposition.match(/filename="([^"]+)"/);
      const filename = match?.[1] ?? `senn_backup_${new Date().toISOString().slice(0, 19).replace(/[:T]/g, '_')}.sql.gz`;

      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);

      toast.success('バックアップファイルをダウンロードしました');
    } catch (e) {
      toast.error(e instanceof Error ? e.message : 'バックアップに失敗しました');
    } finally {
      setExporting(false);
    }
  };

  return (
    <section className="settings__section" data-testid="admin-backup-section">
      <h2 className="settings__section-title">バックアップ</h2>
      <div className="settings__card">
        <p className="admin__hint" style={{ marginBottom: '1rem' }}>
          PostgreSQL の SQL ダンプを gzip 圧縮してダウンロードします。
          リストア（復元）は画面からは行いません（ops 手順で実施）。
          サーバー作業ディレクトリに一時ファイルは作成しません。
        </p>
        <button
          type="button"
          className="admin__btn admin__btn--primary"
          disabled={exporting}
          onClick={() => void handleExport()}
        >
          {exporting ? 'エクスポート中…' : '⚡ 即時バックアップ（SQLダンプ）を実行'}
        </button>
      </div>
    </section>
  );
}
