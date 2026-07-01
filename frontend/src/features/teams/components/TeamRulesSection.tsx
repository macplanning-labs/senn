/**
 * TeamRulesSection.tsx — チームルール管理セクション
 *
 * TeamDetailModal内またはTeamsPage内で使用。
 * ルールの CRUD + Markdown プレビュー。
 */

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import type { TeamRule } from '@/shared/api/types';

interface TeamRulesSectionProps {
  teamId: number;
}

export function TeamRulesSection({ teamId }: TeamRulesSectionProps) {
  const queryClient = useQueryClient();
  const [showEditor, setShowEditor] = useState(false);
  const [editingRule, setEditingRule] = useState<TeamRule | null>(null);
  const [title, setTitle] = useState('');
  const [content, setContent] = useState('');
  const [category, setCategory] = useState('');

  const { data: rules, isLoading } = useQuery<TeamRule[]>({
    queryKey: ['team-rules', teamId],
    queryFn: async () => {
      const res = await apiClient.get<{ results: TeamRule[] } | TeamRule[]>(
        '/team-rules/',
        { params: { team: teamId } },
      );
      return Array.isArray(res.data) ? res.data : res.data.results;
    },
  });

  const saveMutation = useMutation({
    mutationFn: async (data: { team: number; title: string; content: string; category: string }) => {
      if (editingRule) {
        await apiClient.patch(`/team-rules/${editingRule.id}/`, data);
      } else {
        await apiClient.post('/team-rules/', data);
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['team-rules', teamId] });
      resetEditor();
    },
  });

  const deleteMutation = useMutation({
    mutationFn: async (id: number) => {
      await apiClient.delete(`/team-rules/${id}/`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['team-rules', teamId] });
    },
  });

  function resetEditor() {
    setShowEditor(false);
    setEditingRule(null);
    setTitle('');
    setContent('');
    setCategory('');
  }

  function startEdit(rule: TeamRule) {
    setEditingRule(rule);
    setTitle(rule.title);
    setContent(rule.content);
    setCategory(rule.category);
    setShowEditor(true);
  }

  function handleSave() {
    if (!title.trim()) return;
    saveMutation.mutate({ team: teamId, title, content, category });
  }

  const cardStyle: React.CSSProperties = {
    background: 'var(--color-bg-elevated)',
    border: '1px solid var(--color-border-default)',
    borderRadius: 'var(--radius-lg)',
    padding: 'var(--space-4)',
  };

  const inputStyle: React.CSSProperties = {
    width: '100%',
    padding: 'var(--space-2)',
    background: 'var(--color-bg-tertiary)',
    border: '1px solid var(--color-border-default)',
    borderRadius: 'var(--radius-md)',
    color: 'var(--color-text-primary)',
    marginBottom: 'var(--space-2)',
  };

  return (
    <div style={{ marginTop: 'var(--space-4)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 'var(--space-3)' }}>
        <h3 style={{ fontSize: 'var(--font-size-base)', fontWeight: 'var(--font-weight-semibold)', color: 'var(--color-text-primary)' }}>
          📖 チームルール
        </h3>
        <button
          onClick={() => { resetEditor(); setShowEditor(true); }}
          style={{
            padding: '4px 12px', background: 'var(--color-accent-primary)',
            color: 'white', border: 'none', borderRadius: 'var(--radius-md)',
            cursor: 'pointer', fontSize: 'var(--font-size-sm)',
          }}
        >
          + 追加
        </button>
      </div>

      {/* エディタ */}
      {showEditor && (
        <div style={{ ...cardStyle, marginBottom: 'var(--space-3)' }}>
          <input
            placeholder="ルール名"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            style={inputStyle}
          />
          <input
            placeholder="カテゴリ（例: 命名規約、デプロイ手順）"
            value={category}
            onChange={(e) => setCategory(e.target.value)}
            style={inputStyle}
          />
          <textarea
            placeholder="内容（Markdown対応）"
            value={content}
            onChange={(e) => setContent(e.target.value)}
            rows={6}
            style={{ ...inputStyle, resize: 'vertical', fontFamily: 'var(--font-family-mono)', fontSize: 'var(--font-size-sm)' }}
          />
          <div style={{ display: 'flex', gap: 'var(--space-2)', justifyContent: 'flex-end' }}>
            <button
              onClick={resetEditor}
              style={{
                padding: '4px 12px', background: 'transparent',
                border: '1px solid var(--color-border-default)',
                borderRadius: 'var(--radius-md)', cursor: 'pointer',
                color: 'var(--color-text-secondary)',
              }}
            >
              キャンセル
            </button>
            <button
              onClick={handleSave}
              disabled={!title.trim() || saveMutation.isPending}
              style={{
                padding: '4px 12px', background: 'var(--color-accent-primary)',
                color: 'white', border: 'none', borderRadius: 'var(--radius-md)',
                cursor: 'pointer',
              }}
            >
              {saveMutation.isPending ? '保存中...' : editingRule ? '更新' : '作成'}
            </button>
          </div>
        </div>
      )}

      {/* ルール一覧 */}
      {isLoading ? (
        <div style={{ color: 'var(--color-text-tertiary)', textAlign: 'center', padding: 'var(--space-4)' }}>
          読み込み中...
        </div>
      ) : (rules ?? []).length === 0 ? (
        <div style={{ color: 'var(--color-text-tertiary)', textAlign: 'center', padding: 'var(--space-4)' }}>
          ルールはまだありません
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
          {(rules ?? []).map((rule) => (
            <div key={rule.id} style={cardStyle}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
                <div style={{ flex: 1 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)', marginBottom: 'var(--space-1)' }}>
                    <span style={{
                      fontWeight: 'var(--font-weight-semibold)',
                      color: 'var(--color-text-primary)',
                    }}>
                      {rule.title}
                    </span>
                    {rule.category && (
                      <span style={{
                        padding: '1px 6px', borderRadius: 'var(--radius-full)',
                        fontSize: 'var(--font-size-xs)', background: 'var(--color-bg-tertiary)',
                        color: 'var(--color-text-tertiary)',
                      }}>
                        {rule.category}
                      </span>
                    )}
                  </div>
                  {rule.content && (
                    <pre style={{
                      fontSize: 'var(--font-size-sm)', color: 'var(--color-text-secondary)',
                      whiteSpace: 'pre-wrap', lineHeight: 1.6, margin: 0,
                      fontFamily: 'inherit',
                    }}>
                      {rule.content.length > 200 ? rule.content.slice(0, 200) + '...' : rule.content}
                    </pre>
                  )}
                </div>
                <div style={{ display: 'flex', gap: 4, marginLeft: 'var(--space-2)' }}>
                  <button
                    onClick={() => startEdit(rule)}
                    style={{
                      padding: '2px 8px', background: 'var(--color-bg-tertiary)',
                      border: '1px solid var(--color-border-default)',
                      borderRadius: 'var(--radius-sm)', cursor: 'pointer',
                      fontSize: 'var(--font-size-xs)', color: 'var(--color-text-secondary)',
                    }}
                  >
                    ✏️
                  </button>
                  <button
                    onClick={() => { if (confirm('削除しますか？')) deleteMutation.mutate(rule.id); }}
                    style={{
                      padding: '2px 8px', background: 'var(--color-bg-tertiary)',
                      border: '1px solid var(--color-border-default)',
                      borderRadius: 'var(--radius-sm)', cursor: 'pointer',
                      fontSize: 'var(--font-size-xs)', color: '#ef4444',
                    }}
                  >
                    🗑️
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
