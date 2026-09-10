/**
 * TriageRequestsPage.tsx — トリアージ依頼一覧 + 作成 + 承認/却下UI
 *
 * URL: /triage
 * 開発標準書: モーダル編集方式準拠
 */

import { useState } from 'react';
import {
  useTriageRequests,
  useCreateTriageRequest,
  useApproveTriageRequest,
  useRejectTriageRequest,
} from '../hooks/useTriageRequests';
import { useProject } from '@/shared/hooks/useProject';
import { FilterBar } from '@/shared/components/ui/FilterBar';
import type { TriageRequest, TriageStatus } from '@/shared/api/types';

const STATUS_BADGES: Record<TriageStatus, { label: string; color: string; bg: string }> = {
  pending: { label: '承認待ち', color: '#f59e0b', bg: 'rgba(245,158,11,0.15)' },
  approved: { label: '承認済み', color: '#10b981', bg: 'rgba(16,185,129,0.15)' },
  rejected: { label: '却下', color: '#ef4444', bg: 'rgba(239,68,68,0.15)' },
};

const CHANGE_TYPE_LABELS: Record<string, string> = {
  text_request: '📝 テキスト依頼',
  master_change: '🗄️ マスタ変更',
};

export function TriageRequestsPage() {
  const [filter, setFilter] = useState<TriageStatus | ''>('');
  const [search, setSearch] = useState('');
  const [showCreate, setShowCreate] = useState(false);
  const [reviewingId, setReviewingId] = useState<number | null>(null);
  const [reviewComment, setReviewComment] = useState('');

  const { data: requests, isLoading } = useTriageRequests(filter || undefined);
  const createMutation = useCreateTriageRequest();
  const approveMutation = useApproveTriageRequest();
  const rejectMutation = useRejectTriageRequest();

  const [newTitle, setNewTitle] = useState('');
  const [newDesc, setNewDesc] = useState('');
  const [newType, setNewType] = useState('text_request');
  const [approveProjectId, setApproveProjectId] = useState<number | null>(null);

  // プロジェクト一覧取得
  const { projectList: projects } = useProject();

  function handleCreate() {
    if (!newTitle.trim()) return;
    createMutation.mutate(
      { title: newTitle, description: newDesc, change_type: newType },
      {
        onSuccess: () => {
          setShowCreate(false);
          setNewTitle('');
          setNewDesc('');
        },
      },
    );
  }

  function handleApprove(id: number) {
    approveMutation.mutate({
      id,
      comment: reviewComment,
      project_id: approveProjectId ?? undefined,
    }, {
      onSuccess: () => {
        setReviewingId(null);
        setReviewComment('');
        setApproveProjectId(null);
      },
    });
  }

  function handleReject(id: number) {
    rejectMutation.mutate({ id, comment: reviewComment }, {
      onSuccess: () => { setReviewingId(null); setReviewComment(''); },
    });
  }

  return (
    <div style={{ padding: 'var(--space-6)', maxWidth: 900, margin: '0 auto' }}>
      {/* ヘッダー */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 'var(--space-5)' }}>
        <h1 style={{ fontSize: 'var(--font-size-xl)', fontWeight: 'var(--font-weight-bold)', color: 'var(--color-text-primary)' }}>
          📋 Triage Requests
        </h1>
        <button
          onClick={() => setShowCreate(true)}
          style={{
            padding: 'var(--space-2) var(--space-4)',
            background: 'var(--color-accent-primary)',
            color: 'white',
            border: 'none',
            borderRadius: 'var(--radius-md)',
            cursor: 'pointer',
            fontWeight: 'var(--font-weight-semibold)',
          }}
          data-testid="create-triage-btn"
        >
          + 新規依頼
        </button>
      </div>

      {/* フィルタバー（共有コンポーネント） */}
      <FilterBar
        searchValue={search}
        onSearchChange={setSearch}
        searchPlaceholder="依頼を検索..."
        testId="triage-filters"
      >
        {(['', 'pending', 'approved', 'rejected'] as const).map((s) => (
          <button
            key={s}
            onClick={() => setFilter(s)}
            style={{
              padding: 'var(--space-1) var(--space-3)',
              background: filter === s ? 'var(--color-accent-primary)' : 'var(--color-bg-elevated)',
              color: filter === s ? 'white' : 'var(--color-text-secondary)',
              border: '1px solid var(--color-border-default)',
              borderRadius: 'var(--radius-full)',
              cursor: 'pointer',
              fontSize: 'var(--font-size-sm)',
            }}
            data-testid={`status-filter-${s}`}
          >
            {s === '' ? 'すべて' : STATUS_BADGES[s].label}
          </button>
        ))}
      </FilterBar>

      {/* 作成モーダル */}
      {showCreate && (
        <div style={{
          background: 'var(--color-bg-elevated)',
          border: '1px solid var(--color-border-default)',
          borderRadius: 'var(--radius-lg)',
          padding: 'var(--space-5)',
          marginBottom: 'var(--space-4)',
        }}>
          <h3 style={{ marginBottom: 'var(--space-3)', color: 'var(--color-text-primary)' }}>新規依頼</h3>
          <select
            value={newType}
            onChange={(e) => setNewType(e.target.value)}
            style={{
              width: '100%', padding: 'var(--space-2)', marginBottom: 'var(--space-2)',
              background: 'var(--color-bg-tertiary)', border: '1px solid var(--color-border-default)',
              borderRadius: 'var(--radius-md)', color: 'var(--color-text-primary)',
            }}
          >
            <option value="text_request">📝 テキスト依頼</option>
            <option value="master_change">🗄️ マスタ変更</option>
          </select>
          <input
            placeholder="タイトル"
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            style={{
              width: '100%', padding: 'var(--space-2)', marginBottom: 'var(--space-2)',
              background: 'var(--color-bg-tertiary)', border: '1px solid var(--color-border-default)',
              borderRadius: 'var(--radius-md)', color: 'var(--color-text-primary)',
            }}
            data-testid="triage-title-input"
          />
          <textarea
            placeholder="詳細・理由"
            value={newDesc}
            onChange={(e) => setNewDesc(e.target.value)}
            rows={3}
            style={{
              width: '100%', padding: 'var(--space-2)', marginBottom: 'var(--space-3)',
              background: 'var(--color-bg-tertiary)', border: '1px solid var(--color-border-default)',
              borderRadius: 'var(--radius-md)', color: 'var(--color-text-primary)', resize: 'vertical',
            }}
          />
          <div style={{ display: 'flex', gap: 'var(--space-2)', justifyContent: 'flex-end' }}>
            <button
              onClick={() => setShowCreate(false)}
              style={{
                padding: 'var(--space-2) var(--space-3)',
                background: 'transparent', border: '1px solid var(--color-border-default)',
                borderRadius: 'var(--radius-md)', color: 'var(--color-text-secondary)', cursor: 'pointer',
              }}
            >
              キャンセル
            </button>
            <button
              onClick={handleCreate}
              disabled={!newTitle.trim() || createMutation.isPending}
              style={{
                padding: 'var(--space-2) var(--space-3)',
                background: 'var(--color-accent-primary)', color: 'white',
                border: 'none', borderRadius: 'var(--radius-md)', cursor: 'pointer',
              }}
              data-testid="triage-submit-btn"
            >
              {createMutation.isPending ? '送信中...' : '依頼を作成'}
            </button>
          </div>
        </div>
      )}

      {/* 一覧 */}
      {isLoading ? (
        <div style={{ textAlign: 'center', padding: 'var(--space-8)', color: 'var(--color-text-tertiary)' }}>
          読み込み中...
        </div>
      ) : (
        (() => {
          // Frontend search filtering
          const filteredRequests = (requests ?? []).filter(req =>
            !search ||
            req.title.toLowerCase().includes(search.toLowerCase()) ||
            (req.description?.toLowerCase() ?? '').includes(search.toLowerCase())
          );
          return filteredRequests.length === 0 ? (
            <div style={{ textAlign: 'center', padding: 'var(--space-8)', color: 'var(--color-text-tertiary)' }}>
              📋 {search ? `「${search}」に一致する依頼はありません` : '依頼はありません'}
            </div>
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
              {filteredRequests.map((req: TriageRequest) => {
            const badge = STATUS_BADGES[req.status];
            return (
              <div
                key={req.id}
                style={{
                  background: 'var(--color-bg-elevated)',
                  border: '1px solid var(--color-border-default)',
                  borderRadius: 'var(--radius-lg)',
                  padding: 'var(--space-4)',
                }}
                data-testid={`triage-item-${req.id}`}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
                  <div style={{ flex: 1 }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)', marginBottom: 'var(--space-1)' }}>
                      <span style={{
                        padding: '2px 8px', borderRadius: 'var(--radius-full)',
                        fontSize: 'var(--font-size-xs)', fontWeight: 'var(--font-weight-semibold)',
                        color: badge.color, background: badge.bg,
                      }}>
                        {badge.label}
                      </span>
                      <span style={{ fontSize: 'var(--font-size-xs)', color: 'var(--color-text-tertiary)' }}>
                        {CHANGE_TYPE_LABELS[req.changeType] ?? req.changeType}
                      </span>
                      {req.ticketKey && (
                        <span style={{
                          fontSize: 'var(--font-size-xs)', color: 'var(--color-accent-primary)',
                          fontFamily: 'var(--font-family-mono)',
                        }}>
                          {req.ticketKey}
                        </span>
                      )}
                    </div>
                    <div style={{
                      fontSize: 'var(--font-size-base)', fontWeight: 'var(--font-weight-semibold)',
                      color: 'var(--color-text-primary)', marginBottom: 'var(--space-1)',
                    }}>
                      {req.title}
                    </div>
                    {req.description && (
                      <div style={{
                        fontSize: 'var(--font-size-sm)', color: 'var(--color-text-secondary)',
                        lineHeight: 1.5, marginBottom: 'var(--space-2)',
                      }}>
                        {req.description}
                      </div>
                    )}
                    <div style={{ fontSize: 'var(--font-size-xs)', color: 'var(--color-text-tertiary)' }}>
                      {req.requestedBy?.displayName ?? req.requestedBy?.username} · {new Date(req.createdAt).toLocaleDateString('ja-JP')}
                      {req.reviewedBy && (
                        <> · レビュー: {req.reviewedBy.displayName ?? req.reviewedBy.username}</>
                      )}
                    </div>
                    {req.reviewComment && (
                      <div style={{
                        marginTop: 'var(--space-2)', padding: 'var(--space-2)',
                        background: 'var(--color-bg-tertiary)', borderRadius: 'var(--radius-md)',
                        fontSize: 'var(--font-size-sm)', color: 'var(--color-text-secondary)',
                      }}>
                        💬 {req.reviewComment}
                      </div>
                    )}
                    {/* 承認済み→チケットリンク */}
                    {req.status === 'approved' && req.ticketKey && (
                      <div style={{
                        marginTop: 'var(--space-2)', padding: 'var(--space-2)',
                        background: 'rgba(16,185,129,0.08)', borderRadius: 'var(--radius-md)',
                        fontSize: 'var(--font-size-sm)',
                      }}>
                        🎫 <a
                          href={`/p/${req.ticketKey.split('-')[0]}/tickets/${req.ticketKey}`}
                          style={{ color: 'var(--color-accent-primary)', fontWeight: 600 }}
                        >
                          {req.ticketKey}
                        </a> が自動生成されました
                      </div>
                    )}
                  </div>

                  {/* 承認/却下ボタン */}
                  {req.status === 'pending' && (
                    <div style={{ display: 'flex', gap: 'var(--space-1)', marginLeft: 'var(--space-3)' }}>
                      {reviewingId === req.id ? (
                        <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-1)' }}>
                          <input
                            placeholder="コメント（任意）"
                            value={reviewComment}
                            onChange={(e) => setReviewComment(e.target.value)}
                            style={{
                              padding: '4px 8px', fontSize: 'var(--font-size-xs)',
                              background: 'var(--color-bg-tertiary)', border: '1px solid var(--color-border-default)',
                              borderRadius: 'var(--radius-sm)', color: 'var(--color-text-primary)', width: 160,
                            }}
                          />
                          <select
                            value={approveProjectId ?? ''}
                            onChange={(e) => setApproveProjectId(e.target.value ? Number(e.target.value) : null)}
                            style={{
                              padding: '4px 8px', fontSize: 'var(--font-size-xs)',
                              background: 'var(--color-bg-tertiary)', border: '1px solid var(--color-border-default)',
                              borderRadius: 'var(--radius-sm)', color: 'var(--color-text-primary)', width: 160,
                            }}
                          >
                            <option value="">起票先プロジェクト</option>
                            {projects.map((p) => (
                              <option key={p.id} value={p.id}>{p.prefix} — {p.name}</option>
                            ))}
                          </select>
                          <div style={{ display: 'flex', gap: 4 }}>
                            <button
                              onClick={() => handleApprove(req.id)}
                              disabled={!approveProjectId}
                              style={{
                                padding: '2px 8px', background: approveProjectId ? '#10b981' : '#888',
                                color: 'white',
                                border: 'none', borderRadius: 'var(--radius-sm)', cursor: approveProjectId ? 'pointer' : 'not-allowed',
                                fontSize: '0.75rem',
                              }}
                            >
                              ✓ 承認
                            </button>
                            <button
                              onClick={() => handleReject(req.id)}
                              style={{
                                padding: '2px 8px', background: '#ef4444', color: 'white',
                                border: 'none', borderRadius: 'var(--radius-sm)', cursor: 'pointer', fontSize: '0.75rem',
                              }}
                            >
                              ✕ 却下
                            </button>
                          </div>
                        </div>
                      ) : (
                        <button
                          onClick={() => setReviewingId(req.id)}
                          style={{
                            padding: '4px 12px', background: 'var(--color-bg-tertiary)',
                            border: '1px solid var(--color-border-default)',
                            borderRadius: 'var(--radius-md)', cursor: 'pointer',
                            fontSize: 'var(--font-size-xs)', color: 'var(--color-text-secondary)',
                          }}
                        >
                          レビュー
                        </button>
                      )}
                    </div>
                  )}
                </div>
              </div>
                );
              })}
            </div>
          );
        })()
      )}
    </div>
  );
}
