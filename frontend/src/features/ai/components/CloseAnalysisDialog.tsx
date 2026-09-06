/**
 * CloseAnalysisDialog.tsx — チケットクローズ時のAI分析ダイアログ
 *
 * チケットを完了/クローズする際に自動でAI分析を実行し、
 * 技術的負債・将来の課題を抽出。ワンクリックでTeamRuleやBacklogチケットを作成できる。
 */
import { useEffect, useState } from 'react';
import { useCloseAnalysis } from '../useAIAnalysis';
import { apiClient } from '@/shared/api/client';
import type { CloseAnalysisResult } from '@/shared/api/types';
import { useQueryClient } from '@tanstack/react-query';

interface Props {
  ticketId: number;
  projectId: number;
  onClose: () => void;
  isOpen: boolean;
}

export function CloseAnalysisDialog({ ticketId, projectId, onClose, isOpen }: Props) {
  const [result, setResult] = useState<CloseAnalysisResult | null>(null);
  const [createdItems, setCreatedItems] = useState<Set<string>>(new Set());
  const mutation = useCloseAnalysis();
  const queryClient = useQueryClient();

  useEffect(() => {
    if (isOpen && !result && !mutation.isPending) {
      mutation.mutate(ticketId, {
        onSuccess: (data) => setResult(data),
      });
    }
  }, [isOpen, ticketId]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!isOpen) return null;

  const handleCreateBacklogTicket = async (title: string, description: string, index: number) => {
    try {
      await apiClient.post('/tickets/', {
        title,
        description,
        project: projectId,
        status: 'backlog',
      });
      setCreatedItems((prev) => new Set(prev).add(`ticket-${index}`));
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
    } catch {
      // グローバルエラーハンドラで処理
    }
  };

  const handleCreateWikiDraft = async () => {
    if (!result?.wiki_draft) return;
    try {
      await apiClient.post('/team-rules/', {
        title: result.wiki_draft.title,
        content: result.wiki_draft.content_markdown,
        category: result.wiki_draft.category,
      });
      setCreatedItems((prev) => new Set(prev).add('wiki'));
      void queryClient.invalidateQueries({ queryKey: ['team-rules'] });
    } catch {
      // グローバルエラーハンドラで処理
    }
  };

  const overlayStyle: React.CSSProperties = {
    position: 'fixed', inset: 0,
    background: 'rgba(0,0,0,0.5)', zIndex: 1000,
    display: 'flex', alignItems: 'center', justifyContent: 'center',
  };

  const dialogStyle: React.CSSProperties = {
    background: 'var(--color-bg-elevated, #1f1f1f)',
    color: 'var(--color-text-primary)',
    border: '1px solid var(--color-border-default, #2a2a2a)',
    borderRadius: '12px', padding: '1.5rem',
    maxWidth: '560px', width: '90vw',
    maxHeight: '80vh', overflowY: 'auto',
    boxShadow: '0 20px 60px rgba(0,0,0,0.3)',
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={dialogStyle} onClick={(e) => e.stopPropagation()}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
          <h3 style={{ margin: 0, fontSize: '1rem' }}>🤖 クローズ分析</h3>
          <button
            onClick={onClose}
            style={{ background: 'none', border: 'none', fontSize: '1.25rem', cursor: 'pointer', opacity: 0.6 }}
          >
            ✕
          </button>
        </div>

        {mutation.isPending && (
          <div style={{ textAlign: 'center', padding: '2rem', opacity: 0.6 }}>
            <p>AI分析中...</p>
          </div>
        )}

        {mutation.isError && (
          <p style={{ color: 'var(--color-error, #e54d4d)', fontSize: '0.875rem' }}>
            AI分析に失敗しました。ダイアログを閉じて再試行してください。
          </p>
        )}

        {result && !result.has_future_challenges && (
          <div style={{ textAlign: 'center', padding: '1.5rem' }}>
            <p style={{ fontSize: '1.25rem', marginBottom: '0.5rem' }}>✅</p>
            <p style={{ fontSize: '0.875rem', opacity: 0.7 }}>
              将来の課題や技術的負債は検出されませんでした。
            </p>
          </div>
        )}

        {result && result.has_future_challenges && (
          <div>
            {/* Wiki ドラフト */}
            {result.wiki_draft && (
              <div style={{
                padding: '0.75rem', borderRadius: '8px', marginBottom: '1rem',
                background: 'rgba(92, 108, 255, 0.12)',
                border: '1px solid rgba(92, 108, 255, 0.4)',
              }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <div>
                    <span style={{ fontSize: '0.75rem', opacity: 0.7 }}>📝 Wiki ドラフト</span>
                    <p style={{ margin: '0.25rem 0 0', fontWeight: 600, fontSize: '0.875rem' }}>
                      {result.wiki_draft.title}
                    </p>
                  </div>
                  <button
                    onClick={handleCreateWikiDraft}
                    disabled={createdItems.has('wiki')}
                    style={{
                      padding: '0.375rem 0.625rem', borderRadius: '6px',
                      border: 'none', fontSize: '0.75rem', cursor: 'pointer',
                      background: createdItems.has('wiki') ? '#666' : 'var(--color-accent-primary, #5c6cff)',
                      color: '#fff',
                    }}
                  >
                    {createdItems.has('wiki') ? '✓ 作成済み' : 'TeamRule作成'}
                  </button>
                </div>
                <p style={{ margin: '0.5rem 0 0', fontSize: '0.8125rem', whiteSpace: 'pre-wrap', opacity: 0.85 }}>
                  {result.wiki_draft.content_markdown.slice(0, 300)}
                  {result.wiki_draft.content_markdown.length > 300 ? '...' : ''}
                </p>
              </div>
            )}

            {/* バックログチケット候補 */}
            {result.suggested_backlog_tickets.length > 0 && (
              <div>
                <h4 style={{ fontSize: '0.8125rem', margin: '0 0 0.5rem', opacity: 0.7 }}>
                  📋 推奨バックログチケット ({result.suggested_backlog_tickets.length}件)
                </h4>
                {result.suggested_backlog_tickets.map((t, i) => (
                  <div
                    key={i}
                    style={{
                      padding: '0.625rem', borderRadius: '6px', marginBottom: '0.5rem',
                      background: 'var(--color-bg-tertiary, #1a1a1a)',
                      border: '1px solid var(--color-border-default, #2a2a2a)',
                      display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start',
                      gap: '0.75rem',
                    }}
                  >
                    <div style={{ flex: 1 }}>
                      <p style={{ margin: 0, fontWeight: 600, fontSize: '0.8125rem' }}>{t.title}</p>
                      <p style={{ margin: '0.25rem 0 0', fontSize: '0.75rem', opacity: 0.7 }}>
                        {t.description}
                      </p>
                    </div>
                    <button
                      onClick={() => handleCreateBacklogTicket(t.title, t.description, i)}
                      disabled={createdItems.has(`ticket-${i}`)}
                      data-testid={`create-backlog-ticket-${i}`}
                      style={{
                        padding: '0.375rem 0.625rem', borderRadius: '6px',
                        border: 'none', fontSize: '0.75rem', cursor: 'pointer',
                        background: createdItems.has(`ticket-${i}`) ? '#666' : 'var(--color-success, #50e3c2)',
                        color: '#fff', whiteSpace: 'nowrap', flexShrink: 0,
                      }}
                    >
                      {createdItems.has(`ticket-${i}`) ? '✓ 起票済み' : '起票する'}
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        <div style={{ marginTop: '1rem', textAlign: 'right' }}>
          <button
            onClick={onClose}
            style={{
              padding: '0.5rem 1rem', borderRadius: '6px',
              border: '1px solid var(--color-border-default, #2a2a2a)',
              background: 'transparent', color: 'var(--color-text-primary)', cursor: 'pointer',
              fontSize: '0.8125rem',
            }}
          >
            閉じる
          </button>
        </div>
      </div>
    </div>
  );
}
