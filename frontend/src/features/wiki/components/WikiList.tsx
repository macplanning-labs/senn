/**
 * WikiList.tsx — Wiki一覧 + 詳細表示
 *
 * 左カラム: ページ一覧（カテゴリフィルタ + 検索）
 * 右カラム: 選択したページのMarkdownプレビュー
 */

import { useState, useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useSearchParams, useParams } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { FilterBar } from '@/shared/components/ui/FilterBar';
import { useProject } from '@/shared/hooks/useProject';
import { useTeam } from '@/shared/hooks/useTeam';
import './WikiList.css';
import { QuickCreateTicketButton } from '@/features/tickets/components/QuickCreateTicketButton';
import type { LinkedTicketSummary } from '@/shared/api/types';
interface WikiPage {
  id: number;
  title: string;
  slug: string;
  category: string;
  project?: number;
  team?: number;
  content: string;
  renderedContent: string;
  author: { id: number; displayName: string; username: string };
  lastEditor: { id: number; displayName: string; username: string } | null;
  updatedAt: string;
  linkedTickets?: LinkedTicketSummary[];
}

interface WikiListItem {
  id: number;
  title: string;
  slug: string;
  category: string;
  project?: number;
  team?: number;
  author: { id: number; displayName: string; username: string };
  lastEditor: { id: number; displayName: string; username: string } | null;
  revisionCount: number;
  updatedAt: string;
}

const CATEGORIES = [
  { value: '', label: 'All', icon: '📄' },
  { value: 'manual', label: 'Manual', icon: '📘' },
  { value: 'minutes', label: 'Minutes', icon: '📋' },
  { value: 'spec', label: 'Spec', icon: '📐' },
  { value: 'knowhow', label: 'Know-how', icon: '💡' },
  { value: 'glossary', label: 'Glossary', icon: '📚' },
  { value: 'other', label: 'Other', icon: '📄' },
] as const;

function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export function WikiList() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [searchParams] = useSearchParams();
  const { projectKey, teamSlug } = useParams<{ projectKey?: string; teamSlug?: string }>();
  const { currentProject } = useProject();
  const { currentTeam } = useTeam();
  const projectId = currentProject?.id;
  const teamId = currentTeam?.id;

  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [categoryFilter, setCategoryFilter] = useState('');
  const [search, setSearch] = useState('');
  const [isEditing, setIsEditing] = useState(false);
  const [editTitle, setEditTitle] = useState('');
  const [editContent, setEditContent] = useState('');
  const [editCategory, setEditCategory] = useState('other');
  const [isCreating, setIsCreating] = useState(false);
  const [linkCopied, setLinkCopied] = useState(false);

  // --- D&D インポート ---
  const [isDragOver, setIsDragOver] = useState(false);
  const [importResult, setImportResult] = useState<{ success: number; failed: string[] } | null>(null);
  const dragCounter = useRef(0);

  // ページ一覧（API は project/team とも ID。slug/prefix を送ると 400 になる）
  const { data: pages, isLoading } = useQuery<{ results: WikiListItem[] }>({
    queryKey: ['wiki-pages', categoryFilter, search, projectId, teamId],
    queryFn: async () => {
      const params: Record<string, string | number> = {};
      if (categoryFilter) params.category = categoryFilter;
      if (search) params.search = search;
      if (projectId) params.project = projectId;
      if (teamId) params.team = teamId;
      const res = await apiClient.get<{ results: WikiListItem[] }>('/wiki/', { params });
      return res.data;
    },
    enabled: !!(projectId || teamId) || (!projectKey && !teamSlug),
  });

  // ページ詳細
  const { data: selectedPage } = useQuery<WikiPage>({
    queryKey: ['wiki-page', selectedId],
    queryFn: async () => {
      const res = await apiClient.get<WikiPage>(`/wiki/${selectedId}/`);
      return res.data;
    },
    enabled: !!selectedId,
  });

  // 作成
  const createMutation = useMutation({
    mutationFn: async (data: { title: string; content: string; category: string }) => {
      const payload: Record<string, string | number> = { ...data };
      if (projectId) payload.project = projectId;
      if (teamId) payload.team = teamId;
      const res = await apiClient.post<WikiPage>('/wiki/', payload);
      return res.data;
    },
    onSuccess: (newPage) => {
      void queryClient.invalidateQueries({ queryKey: ['wiki-pages'] });
      setSelectedId(newPage.id);
      setIsCreating(false);
      setIsEditing(false);
    },
  });

  // 更新
  const updateMutation = useMutation({
    mutationFn: async (data: { title: string; content: string; category: string }) => {
      await apiClient.patch(`/wiki/${selectedId}/`, data);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['wiki-pages'] });
      void queryClient.invalidateQueries({ queryKey: ['wiki-page', selectedId] });
      setIsEditing(false);
    },
  });

  // 削除
  const deleteMutation = useMutation({
    mutationFn: async (id: number) => {
      await apiClient.delete(`/wiki/${id}/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['wiki-pages'] });
      setSelectedId(null);
      setIsEditing(false);
      setIsCreating(false);
    },
    onError: () => {
      alert(t('common.error'));
    },
  });

  const pageList = pages?.results ?? [];

  useEffect(() => {
    const slugParam = searchParams.get('page');
    if (slugParam && !selectedId && pages?.results.length) {
      const match = pages.results.find((p) => p.slug === slugParam);
      if (match) {
        setSelectedId(match.id);
      }
    }
  }, [searchParams, selectedId, pages]);

  function startEdit() {
    if (selectedPage) {
      setEditTitle(selectedPage.title);
      setEditContent(selectedPage.content);
      setEditCategory(selectedPage.category);
      setIsEditing(true);
      setIsCreating(false);
    }
  }

  function startCreate() {
    setEditTitle('');
    setEditContent('');
    setEditCategory('other');
    setIsCreating(true);
    setIsEditing(true);
    setSelectedId(null);
  }

  function handleSave() {
    const data = { title: editTitle, content: editContent, category: editCategory };
    if (isCreating) {
      createMutation.mutate(data);
    } else {
      updateMutation.mutate(data);
    }
  }

  function handleDelete() {
    if (!selectedId || !selectedPage) return;
    if (!window.confirm(t('wiki.deleteConfirm', { title: selectedPage.title }))) return;
    deleteMutation.mutate(selectedId);
  }

  // --- D&D ハンドラー ---
  const handleDragEnter = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounter.current++;
    if (e.dataTransfer.types.includes('Files')) {
      setIsDragOver(true);
    }
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounter.current--;
    if (dragCounter.current === 0) {
      setIsDragOver(false);
    }
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  }, []);

  const handleDrop = useCallback(async (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);
    dragCounter.current = 0;

    const files = Array.from(e.dataTransfer.files).filter(
      (f) => f.name.endsWith('.md') || f.name.endsWith('.markdown') || f.name.endsWith('.txt')
    );

    if (files.length === 0) return;

    let success = 0;
    const failed: string[] = [];

    for (const file of files) {
      try {
        const content = await file.text();
        // ファイル名から拡張子を除去してタイトルにする
        const title = file.name.replace(/\.(md|markdown|txt)$/, '').replace(/[-_]/g, ' ');
        const payload: Record<string, string | number> = {
          title,
          content,
          category: 'other',
        };
        if (projectId) payload.project = projectId;
        if (teamId) payload.team = teamId;
        await apiClient.post('/wiki/', payload);
        success++;
      } catch {
        failed.push(file.name);
      }
    }

    // 一覧を再取得
    void queryClient.invalidateQueries({ queryKey: ['wiki-pages'] });

    // 結果表示（3秒後に自動消去）
    setImportResult({ success, failed });
    setTimeout(() => setImportResult(null), 5000);
  }, [queryClient, projectId, teamId]);

  return (
    <div
      className={`wiki ${isDragOver ? 'wiki--drag-over' : ''}`}
      data-testid="wiki-page"
      onDragEnter={handleDragEnter}
      onDragLeave={handleDragLeave}
      onDragOver={handleDragOver}
      onDrop={handleDrop}
    >
      {/* D&D オーバーレイ */}
      {isDragOver && (
        <div className="wiki__drop-overlay">
          <div className="wiki__drop-message">
            <span className="wiki__drop-icon">📥</span>
            <span className="wiki__drop-text">{t('wikiPage.dropImport')}</span>
            <span className="wiki__drop-hint">.md / .markdown / .txt</span>
          </div>
        </div>
      )}

      {/* インポート結果トースト */}
      {importResult && (
        <div className="wiki__import-toast">
          {importResult.success > 0 && (
            <span className="wiki__import-success">
              ✅ {importResult.success}件のページをインポートしました
            </span>
          )}
          {importResult.failed.length > 0 && (
            <span className="wiki__import-failed">
              ❌ 失敗: {importResult.failed.join(', ')}
            </span>
          )}
        </div>
      )}
      <div className="wiki__header">
        <h1 className="wiki__title">{t('nav.wiki')}</h1>
        <button
          className="wiki__create-btn"
          onClick={startCreate}
          data-testid="create-wiki-btn"
        >
          + {t('wiki.create')}
        </button>
      </div>

      <div className="wiki__layout">
        {/* 左: ページ一覧 */}
        <aside className="wiki__sidebar" data-testid="wiki-sidebar">
          <FilterBar
            searchValue={search}
            onSearchChange={setSearch}
            searchPlaceholder={t('common.search')}
            testId="wiki-filters"
          >
            <div className="wiki__categories">
              {CATEGORIES.map((cat) => (
                <button
                  key={cat.value}
                  className={`wiki__cat-btn ${categoryFilter === cat.value ? 'wiki__cat-btn--active' : ''}`}
                  onClick={() => setCategoryFilter(cat.value)}
                  data-testid={`category-filter-${cat.value}`}
                >
                  {cat.icon} {cat.label}
                </button>
              ))}
            </div>
          </FilterBar>

          <div className="wiki__page-list">
            {isLoading ? (
              Array.from({ length: 4 }).map((_, i) => (
                <div key={i} className="wiki__skeleton" />
              ))
            ) : !pageList.length ? (
              <div className="wiki__empty-list">No pages found</div>
            ) : (
              pageList.map((page) => (
                <button
                  key={page.id}
                  className={`wiki__page-item ${selectedId === page.id ? 'wiki__page-item--active' : ''}`}
                  onClick={() => { setSelectedId(page.id); setIsEditing(false); setIsCreating(false); }}
                  data-testid={`wiki-item-${page.id}`}
                >
                  <span className="wiki__page-item-title">{page.title}</span>
                  <span className="wiki__page-item-meta">
                    {timeAgo(page.updatedAt)} · {page.revisionCount} revs
                  </span>
                </button>
              ))
            )}
          </div>
        </aside>

        {/* 右: コンテンツ */}
        <main className="wiki__content" data-testid="wiki-content">
          {isEditing ? (
            /* 編集モード */
            <div className="wiki__editor">
              <input
                type="text"
                className="wiki__editor-title"
                value={editTitle}
                onChange={(e) => setEditTitle(e.target.value)}
                placeholder="Page title"
                data-testid="wiki-title-input"
              />
              <select
                className="wiki__editor-category"
                value={editCategory}
                onChange={(e) => setEditCategory(e.target.value)}
                data-testid="wiki-category-input"
              >
                {CATEGORIES.filter((c) => c.value).map((c) => (
                  <option key={c.value} value={c.value}>{c.icon} {c.label}</option>
                ))}
              </select>
              <textarea
                className="wiki__editor-content"
                value={editContent}
                onChange={(e) => setEditContent(e.target.value)}
                placeholder="Write in Markdown..."
                rows={20}
                data-testid="wiki-content-input"
              />
              <div className="wiki__editor-actions">
                <button
                  className="wiki__editor-cancel"
                  onClick={() => { setIsEditing(false); setIsCreating(false); }}
                >
                  {t('common.cancel')}
                </button>
                <button
                  className="wiki__editor-save"
                  onClick={handleSave}
                  disabled={!editTitle.trim() || createMutation.isPending || updateMutation.isPending}
                  data-testid="wiki-save-btn"
                >
                  {t('common.save')}
                </button>
              </div>
            </div>
          ) : selectedPage ? (
            /* 閲覧モード */
            <div className="wiki__viewer">
              <div className="wiki__viewer-header">
                <h2 className="wiki__viewer-title">{selectedPage.title}</h2>
                <div className="wiki__viewer-actions">
                  <button
                    className="wiki__edit-btn"
                    onClick={() => {
                      if (!selectedPage) return;
                      const url = `${window.location.origin}${window.location.pathname}?page=${selectedPage.slug}`;
                      void navigator.clipboard.writeText(url);
                      setLinkCopied(true);
                      setTimeout(() => setLinkCopied(false), 1500);
                    }}
                    aria-label="Copy wiki page link"
                    title={linkCopied ? 'コピーしました' : 'リンクをコピー'}
                    data-testid="wiki-copy-link-btn"
                  >
                    {linkCopied ? '✅' : '🔗'}
                  </button>
                  <button
                    className="wiki__edit-btn"
                    onClick={startEdit}
                    data-testid="wiki-edit-btn"
                  >
                    ✏️ {t('wiki.edit')}
                  </button>
                  <button
                    className="wiki__delete-btn"
                    onClick={handleDelete}
                    disabled={deleteMutation.isPending}
                    data-testid="wiki-delete-btn"
                  >
                    🗑️ {t('common.delete')}
                  </button>
                </div>
              </div>
              <div className="wiki__viewer-meta">
                Last edited by {selectedPage.lastEditor?.displayName ?? selectedPage.author.displayName} · {timeAgo(selectedPage.updatedAt)}
              </div>
              <div
                className="wiki__viewer-body"
                dangerouslySetInnerHTML={{ __html: selectedPage.renderedContent }}
                data-testid="wiki-rendered"
              />

              {/* 紐付きチケット + ワンクリック起票 */}
              <div className="wiki__linked-section" style={{
                marginTop: '1.5rem', paddingTop: '1rem',
                borderTop: '1px solid var(--color-border, #e0e0e0)',
              }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.75rem' }}>
                  <h4 style={{ margin: 0, fontSize: '0.875rem', fontWeight: 600 }}>
                    🎫 紐付きチケット
                    {selectedPage.linkedTickets && selectedPage.linkedTickets.length > 0
                      ? ` (${selectedPage.linkedTickets.length})`
                      : ''
                    }
                  </h4>
                  <QuickCreateTicketButton
                    defaultTitle={`[Wiki] ${selectedPage.title}`}
                    defaultDescription={`Wikiページ「${selectedPage.title}」から起票\n\n---\n\n${selectedPage.content?.slice(0, 500) ?? ''}`}
                    projectId={selectedPage.project ?? 0}
                    wikiPageId={selectedPage.id}
                    label="⚡ チケット起票"
                  />
                </div>
                {selectedPage.linkedTickets && selectedPage.linkedTickets.length > 0 ? (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '0.375rem' }}>
                    {selectedPage.linkedTickets.map((t: { id: number; ticketKey: string; title: string; status: string }) => (
                      <a
                        key={t.id}
                        href={`/project/${t.ticketKey?.split('-')[0] ?? 'XX'}/tickets/${t.ticketKey}`}
                        style={{
                          padding: '0.5rem 0.625rem', borderRadius: '6px',
                          background: 'var(--color-bg-tertiary)',
                          border: '1px solid var(--color-border, #e0e0e0)',
                          color: 'var(--color-text-primary)',
                          fontSize: '0.8125rem', textDecoration: 'none',
                          display: 'flex', alignItems: 'center', gap: '0.5rem',
                        }}
                      >
                        <span style={{ fontWeight: 600, opacity: 0.7 }}>{t.ticketKey}</span>
                        <span>{t.title}</span>
                        <span style={{
                          marginLeft: 'auto', fontSize: '0.75rem',
                          padding: '0.125rem 0.375rem', borderRadius: '4px',
                          background: t.status === 'closed' ? 'rgba(107,114,128,0.15)' : 'rgba(59,130,246,0.15)',
                          color: t.status === 'closed' ? '#6b7280' : '#3b82f6',
                        }}>
                          {t.status}
                        </span>
                      </a>
                    ))}
                  </div>
                ) : (
                  <p style={{ fontSize: '0.8125rem', opacity: 0.5, margin: 0 }}>
                    紐付きチケットはありません
                  </p>
                )}
              </div>
            </div>
          ) : (
            <div className="wiki__placeholder">
              <div className="wiki__placeholder-icon">📥</div>
              <div className="wiki__placeholder-text">{t('wikiPage.placeholder')}</div>
              <div className="wiki__placeholder-hint">{t('wikiPage.placeholderHint')}</div>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
