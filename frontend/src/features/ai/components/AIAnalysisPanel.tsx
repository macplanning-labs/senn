/**
 * AIAnalysisPanel.tsx — チケット詳細のAIコンテキスト分析パネル
 *
 * 「🤖 AI分析」ボタン押下でチケットに紐付くTeamRuleを参照し、
 * ルール違反の検出と実装アドバイスを表示する。
 */
import { useState } from 'react';
import { useContextAnalysis } from '../useAIAnalysis';
import type { ContextAnalysisResult } from '@/shared/api/types';

interface Props {
  ticketId: number;
}

export function AIAnalysisPanel({ ticketId }: Props) {
  const [result, setResult] = useState<ContextAnalysisResult | null>(null);
  const mutation = useContextAnalysis();

  const handleAnalyze = () => {
    mutation.mutate(ticketId, {
      onSuccess: (data) => setResult(data),
    });
  };

  return (
    <div style={{
      marginTop: '1rem',
      padding: '1rem',
      borderRadius: '8px',
      background: 'var(--surface-secondary, #f5f5f5)',
      border: '1px solid var(--border-subtle, #e0e0e0)',
    }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <h4 style={{ margin: 0, fontSize: '0.875rem', fontWeight: 600 }}>
          🤖 AI コンテキスト分析
        </h4>
        <button
          onClick={handleAnalyze}
          disabled={mutation.isPending}
          data-testid="ai-context-analysis-btn"
          style={{
            padding: '0.375rem 0.75rem',
            borderRadius: '6px',
            border: 'none',
            background: 'var(--accent-primary, #5b5bd6)',
            color: '#fff',
            fontSize: '0.8125rem',
            cursor: mutation.isPending ? 'wait' : 'pointer',
            opacity: mutation.isPending ? 0.7 : 1,
          }}
        >
          {mutation.isPending ? '分析中...' : '分析する'}
        </button>
      </div>

      {mutation.isError && (
        <p style={{ color: 'var(--error, #e54d4d)', marginTop: '0.5rem', fontSize: '0.8125rem' }}>
          AI分析に失敗しました。再試行してください。
        </p>
      )}

      {result && (
        <div style={{ marginTop: '0.75rem' }}>
          {/* ルール違反 */}
          {result.rule_violations.length > 0 && (
            <div style={{ marginBottom: '0.75rem' }}>
              <h5 style={{ margin: '0 0 0.375rem', fontSize: '0.8125rem', color: 'var(--warning, #e5a100)' }}>
                ⚠️ ルール違反 ({result.rule_violations.length}件)
              </h5>
              {result.rule_violations.map((v, i) => (
                <div
                  key={i}
                  style={{
                    padding: '0.5rem',
                    marginBottom: '0.25rem',
                    borderRadius: '4px',
                    background: 'var(--surface-warning, #fef3cd)',
                    fontSize: '0.8125rem',
                    color: '#1a1a1a',
                  }}
                >
                  <strong>{v.rule_title}</strong>
                  <p style={{ margin: '0.25rem 0 0', opacity: 0.85 }}>{v.warning}</p>
                </div>
              ))}
            </div>
          )}

          {result.rule_violations.length === 0 && (
            <p style={{ fontSize: '0.8125rem', color: 'var(--success, #2da44e)', marginBottom: '0.5rem' }}>
              ✅ ルール違反はありません
            </p>
          )}

          {/* 実装アドバイス */}
          <div style={{
            padding: '0.5rem',
            borderRadius: '4px',
            background: 'var(--surface-info, #d9ecff)',
            fontSize: '0.8125rem',
            color: '#1a1a1a',
          }}>
            <strong>💡 実装アドバイス</strong>
            <p style={{ margin: '0.25rem 0 0', whiteSpace: 'pre-wrap' }}>
              {result.implementation_hint}
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
