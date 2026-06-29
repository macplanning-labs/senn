/**
 * WikiList.tsx — Wiki一覧 + 詳細表示
 *
 * 左カラム: ページ一覧（カテゴリフィルタ + 検索）
 * 右カラム: 選択したページのMarkdownプレビュー
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import './WikiList.css';

interface WikiPage {
  id: number;
  title: string;
  slug: string;
  category: string;
  content: string;
  renderedContent: string;
  author: { id: number; displayName: string; username: string };
  lastEditor: { id: number; displayName: string; username: string } | null;
  updatedAt: string;
}

interface WikiListItem {
  id: number;
  title: string;
  slug: string;
  category: string;
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

  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [categoryFilter, setCategoryFilter] = useState('');
  const [search, setSearch] = useState('');
  const [isEditing, setIsEditing] = useState(false);
  const [editTitle, setEditTitle] = useState('');
  const [editContent, setEditContent] = useState('');
  const [editCategory, setEditCategory] = useState('other');
  const [isCreating, setIsCreating] = useState(false);

  // ページ一覧
  const { data: pages, isLoading } = useQuery<{ results: WikiListItem[] }>({
    queryKey: ['wiki-pages', categoryFilter, search],
    queryFn: async () => {
      const params: Record<string, string> = {};
      if (categoryFilter) params.category = categoryFilter;
      if (search) params.search = search;
      const res = await apiClient.get<{ results: WikiListItem[] }>('/wiki/', { params });
      return res.data;
    },
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
      const res = await apiClient.post<WikiPage>('/wiki/', data);
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

  const pageList = pages?.results ?? [];

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

  return (
    <div className="wiki" data-testid="wiki-page">
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
          <input
            type="text"
            className="wiki__search"
            placeholder={t('common.search')}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            data-testid="wiki-search"
          />

          <div className="wiki__categories">
            {CATEGORIES.map((cat) => (
              <button
                key={cat.value}
                className={`wiki__cat-btn ${categoryFilter === cat.value ? 'wiki__cat-btn--active' : ''}`}
                onClick={() => setCategoryFilter(cat.value)}
              >
                {cat.icon} {cat.label}
              </button>
            ))}
          </div>

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
                <button
                  className="wiki__edit-btn"
                  onClick={startEdit}
                  data-testid="wiki-edit-btn"
                >
                  ✏️ {t('wiki.edit')}
                </button>
              </div>
              <div className="wiki__viewer-meta">
                Last edited by {selectedPage.lastEditor?.displayName ?? selectedPage.author.displayName} · {timeAgo(selectedPage.updatedAt)}
              </div>
              <div
                className="wiki__viewer-body"
                dangerouslySetInnerHTML={{ __html: selectedPage.renderedContent }}
                data-testid="wiki-rendered"
              />
            </div>
          ) : (
            <div className="wiki__placeholder">
              Select a page or create a new one
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
