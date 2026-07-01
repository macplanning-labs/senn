/**
 * SprintHealthWidget.tsx — AI駆動スプリント健全性ウィジェット
 *
 * ダッシュボードに配置し、AIがスプリントのリスクを分析。
 * Ollama/OpenAIが未接続でも安全なフォールバック値を表示。
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import './SprintHealthWidget.css';

interface SprintHealthData {
  risk_level: 'high' | 'medium' | 'low';
  summary: string;
  alerts: { task_id: number; reason: string }[];
  suggested_actions: string[];
}

interface AiStatusData {
  ollama: { url: string; connected: boolean; model: string };
  openai: { configured: boolean };
}

interface Props {
  data: unknown;
}

export function SprintHealthWidget({ data }: Props) {
  const { t } = useTranslation();
  const widgetData = data as { project_id?: number; cycle_id?: number } | null;
  const [expanded, setExpanded] = useState(false);

  // AI接続ステータス
  const { data: aiStatus } = useQuery<AiStatusData>({
    queryKey: ['ai-status'],
    queryFn: async () => (await apiClient.get('/ai/status/')).data,
    staleTime: 60_000,
  });

  // Sprint Health 分析（手動トリガー）
  const [analyzing, setAnalyzing] = useState(false);
  const [healthResult, setHealthResult] = useState<SprintHealthData | null>(null);

  const analyze = async () => {
    if (!widgetData?.project_id) return;
    setAnalyzing(true);
    try {
      const res = await apiClient.post('/ai/sprint-health/', {
        project_id: widgetData.project_id,
        cycle_id: widgetData.cycle_id,
      });
      setHealthResult(res.data as SprintHealthData);
      setExpanded(true);
    } catch {
      setHealthResult({
        risk_level: 'medium',
        summary: t('ai.unavailable'),
        alerts: [],
        suggested_actions: [],
      });
    } finally {
      setAnalyzing(false);
    }
  };

  const isConnected = aiStatus?.ollama?.connected || aiStatus?.openai?.configured;
  const riskColors = {
    high: '#ef4444',
    medium: '#f59e0b',
    low: '#22c55e',
  };
  const riskIcons = {
    high: '🔴',
    medium: '🟡',
    low: '🟢',
  };

  return (
    <div className="sprint-health">
      {/* ヘッダー: AI接続ステータス */}
      <div className="sprint-health__header">
        <div className="sprint-health__status">
          <span
            className="sprint-health__dot"
            style={{ background: isConnected ? '#22c55e' : '#6b7280' }}
          />
          <span className="sprint-health__status-text">
            {isConnected ? 'AI' : t('ai.disconnected')}
          </span>
          {aiStatus?.ollama?.connected && (
            <span className="sprint-health__model">{aiStatus.ollama.model}</span>
          )}
        </div>
        <button
          className="sprint-health__analyze-btn"
          onClick={analyze}
          disabled={analyzing || !isConnected}
        >
          {analyzing ? t('ai.analyzing') : `🤖 ${t('ai.sprintHealth')}`}
        </button>
      </div>

      {/* 分析結果 */}
      {healthResult && (
        <div className="sprint-health__result">
          <div
            className="sprint-health__risk-badge"
            style={{ background: riskColors[healthResult.risk_level] }}
          >
            {riskIcons[healthResult.risk_level]} {healthResult.risk_level.toUpperCase()}
          </div>
          <p className="sprint-health__summary">{healthResult.summary}</p>

          {expanded && healthResult.alerts.length > 0 && (
            <div className="sprint-health__alerts">
              <h4 className="sprint-health__section-title">⚠️ Alerts</h4>
              {healthResult.alerts.map((a, i) => (
                <div key={i} className="sprint-health__alert-item">
                  <span className="sprint-health__alert-task">#{a.task_id}</span>
                  {a.reason}
                </div>
              ))}
            </div>
          )}

          {expanded && healthResult.suggested_actions.length > 0 && (
            <div className="sprint-health__actions">
              <h4 className="sprint-health__section-title">💡 Actions</h4>
              <ul>
                {healthResult.suggested_actions.map((a, i) => (
                  <li key={i}>{a}</li>
                ))}
              </ul>
            </div>
          )}

          {(healthResult.alerts.length > 0 || healthResult.suggested_actions.length > 0) && (
            <button
              className="sprint-health__toggle"
              onClick={() => setExpanded(!expanded)}
            >
              {expanded ? '▲' : '▼'}
            </button>
          )}
        </div>
      )}

      {/* 未分析時のプレースホルダー */}
      {!healthResult && !analyzing && (
        <div className="sprint-health__placeholder">
          {isConnected
            ? `🤖 ${t('ai.sprintHealth')} — Click to analyze`
            : `⚡ ${t('ai.unavailable')}`}
        </div>
      )}
    </div>
  );
}
