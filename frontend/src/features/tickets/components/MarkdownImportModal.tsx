/**
 * MarkdownImportModal.tsx — Markdownチェックリストインポート
 *
 * - [ ] / - [x] 形式のチェックリストをMarkdownファイルから読み込み
 * - 各行をチケット化
 * - タイトルは最初の100文字、descriptionは全文
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import './MarkdownImportModal.css';

interface MarkdownImportModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess?: () => void;
}

interface TicketData {
  title: string;
  description: string;
  status: string;
  priority: string;
  project: number;
}

/**
 * Markdownテキストからチェックリスト行を抽出
 * 対応形式: - [ ] テキスト or - [x] テキスト（先頭の空白はOK）
 */
function parseMarkdownChecklists(content: string): TicketData[] {
  const regex = /^\s*-\s*\[[ xX]\]\s*(.+)$/gm;
  const tickets: TicketData[] = [];
  let match;

  // eslint-disable-next-line no-cond-assign
  while ((match = regex.exec(content)) !== null) {
    const text = (match[1] ?? '').trim();
    if (text) {
      tickets.push({
        title: text.length > 100 ? text.slice(0, 100) : text,
        description: text,
        status: 'backlog',
        priority: 'medium',
        project: 0, // 後で設定される
      });
    }
  }

  return tickets;
}

export function MarkdownImportModal({ isOpen, onClose, onSuccess }: MarkdownImportModalProps) {
  const { t } = useTranslation();
  const { currentProject } = useProject();
  const queryClient = useQueryClient();
  const [file, setFile] = useState<File | null>(null);
  const [preview, setPreview] = useState<TicketData[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const bulkImportMutation = useMutation({
    mutationFn: async (tickets: TicketData[]) => {
      return apiClient.post('/tickets/bulk-import/', {
        tickets: tickets.map(t => ({
          ...t,
          project: currentProject?.id,
        })),
      });
    },
    onSuccess: () => {
      // チケット一覧を再取得
      queryClient.invalidateQueries({ queryKey: ['tickets'] });
      setFile(null);
      setPreview([]);
      onClose();
      onSuccess?.();
    },
  });

  const handleFileSelect = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const selectedFile = e.target.files?.[0];
    if (!selectedFile) return;

    setFile(selectedFile);
    setIsLoading(true);

    try {
      const content = await selectedFile.text();
      const tickets = parseMarkdownChecklists(content);
      setPreview(tickets);
    } catch (error) {
      console.error('Failed to read file:', error);
      alert('ファイル読み込みに失敗しました。');
    } finally {
      setIsLoading(false);
    }
  };

  const handleImport = () => {
    if (preview.length === 0) {
      alert('インポートするチェックリストがありません。');
      return;
    }
    bulkImportMutation.mutate(preview);
  };

  if (!isOpen) return null;

  return (
    <div className="markdown-import-modal__overlay" onClick={onClose}>
      <div className="markdown-import-modal" onClick={(e) => e.stopPropagation()}>
        <div className="markdown-import-modal__header">
          <h2 className="markdown-import-modal__title">Markdownからインポート</h2>
          <button
            className="markdown-import-modal__close"
            onClick={onClose}
            aria-label="Close"
          >
            ×
          </button>
        </div>

        <div className="markdown-import-modal__body">
          {preview.length === 0 ? (
            <div className="markdown-import-modal__section">
              <label className="markdown-import-modal__label">
                Markdownファイルを選択
              </label>
              <div className="markdown-import-modal__file-input-wrapper">
                <input
                  type="file"
                  accept=".md,.markdown,text/markdown,text/plain"
                  onChange={handleFileSelect}
                  disabled={isLoading}
                  className="markdown-import-modal__file-input"
                />
                {file && (
                  <span className="markdown-import-modal__file-name">
                    {file.name}
                  </span>
                )}
              </div>
              <p className="markdown-import-modal__info">
                - [ ] または - [x] 形式のチェックリストを検出します
              </p>
            </div>
          ) : (
            <div className="markdown-import-modal__section">
              <h3 className="markdown-import-modal__preview-title">
                検出されたチェックリスト ({preview.length}件)
              </h3>
              <div className="markdown-import-modal__preview-list">
                {preview.map((ticket, idx) => (
                  <div key={idx} className="markdown-import-modal__preview-item">
                    <span className="markdown-import-modal__preview-number">
                      {idx + 1}
                    </span>
                    <div className="markdown-import-modal__preview-content">
                      <div className="markdown-import-modal__preview-title">
                        {ticket.title}
                      </div>
                      {ticket.title !== ticket.description && (
                        <div className="markdown-import-modal__preview-desc">
                          {ticket.description}
                        </div>
                      )}
                    </div>
                  </div>
                ))}
              </div>
              <button
                className="markdown-import-modal__back-btn"
                onClick={() => {
                  setFile(null);
                  setPreview([]);
                }}
                disabled={bulkImportMutation.isPending}
              >
                ← 別のファイルを選択
              </button>
            </div>
          )}
        </div>

        <div className="markdown-import-modal__footer">
          <button
            className="markdown-import-modal__btn markdown-import-modal__btn--cancel"
            onClick={onClose}
            disabled={bulkImportMutation.isPending}
          >
            {t('common.cancel')}
          </button>
          {preview.length > 0 && (
            <button
              className="markdown-import-modal__btn markdown-import-modal__btn--import"
              onClick={handleImport}
              disabled={bulkImportMutation.isPending}
            >
              {bulkImportMutation.isPending
                ? 'インポート中...'
                : `${preview.length}件インポート`}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
