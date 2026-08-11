/**
 * BurndownChart.tsx — バーンダウンチャート（SVGラインチャート）
 *
 * 理想線（直線・破線）+ 実績線（折れ線）+ スコープ線（階段状）を表示。
 * 実績が理想を上回る（遅延）場合は赤色で警告。
 * チャートライブラリ不使用でバンドルサイズを抑制。
 */
import { useBurndown } from '../hooks/useCycles';
import './BurndownChart.css';
import { useTranslation } from 'react-i18next';

const CHART_WIDTH = 600;
const CHART_HEIGHT = 240;
const PADDING = { top: 20, right: 30, bottom: 50, left: 50 };

export function BurndownChart({ cycleId }: { cycleId: number }) {
  const { t } = useTranslation();
  const { data: points = [] } = useBurndown(cycleId);

  if (points.length === 0) {
    return (
      <div className="burndown-chart burndown-chart--empty">
        <h3 className="burndown-chart__title">バーンダウン</h3>
        <p className="burndown-chart__hint">
          {t('cycle.burndownEmpty')}
        </p>
      </div>
    );
  }

  const innerW = CHART_WIDTH - PADDING.left - PADDING.right;
  const innerH = CHART_HEIGHT - PADDING.top - PADDING.bottom;
  const maxVal = Math.max(
    ...points.map(p => Math.max(p.ideal, p.actual, p.totalScope)),
    1,
  );

  // 座標計算
  const toX = (i: number) =>
    PADDING.left + (i / Math.max(points.length - 1, 1)) * innerW;
  const toY = (val: number) =>
    PADDING.top + innerH - (val / maxVal) * innerH;

  // SVGパスを生成
  const idealPath = points
    .map((p, i) => `${i === 0 ? 'M' : 'L'}${toX(i)},${toY(p.ideal)}`)
    .join(' ');

  // 実績線（今日以降は表示しない）
  const today = new Date().toISOString().split('T')[0] ?? '';
  const actualPoints = points.filter(p => p.date <= today);
  const actualPath = actualPoints
    .map((p, i) => `${i === 0 ? 'M' : 'L'}${toX(i)},${toY(p.actual)}`)
    .join(' ');

  // スコープ線（階段状）— スコープが変わった日だけ段差がつく
  const scopePath = points
    .map((p, i) => {
      const isFirst = i === 0;
      const prevPoint = i > 0 ? points[i - 1] : null;
      const prevScope = prevPoint ? (prevPoint.totalScope ?? 0) : (p.totalScope ?? 0);
      const currScope = p.totalScope ?? 0;

      if (isFirst) {
        return `M${toX(i)},${toY(currScope)}`;
      } else if (currScope !== prevScope) {
        // スコープが変わった：垂直線（前の値）→ 水平線（現在の値）
        return `L${toX(i)},${toY(prevScope)} L${toX(i)},${toY(currScope)}`;
      } else {
        // スコープが同じ：水平線
        return `L${toX(i)},${toY(currScope)}`;
      }
    })
    .join(' ');

  // 実績が遅延しているか
  const lastActual = actualPoints[actualPoints.length - 1];
  const lastIdealIndex = actualPoints.length - 1;
  const lastIdeal = points[lastIdealIndex];
  const isDelayed = lastActual && lastIdeal && lastActual.actual > lastIdeal.ideal;

  // X軸ラベル（等間隔で最大7個）
  const labelInterval = Math.max(1, Math.floor(points.length / 7));

  return (
    <div className="burndown-chart">
      <h3 className="burndown-chart__title">バーンダウン</h3>
      <svg
        viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`}
        className="burndown-chart__svg"
      >
        {/* Y軸ガイド線 */}
        {[0, 0.25, 0.5, 0.75, 1].map(pct => {
          const y = PADDING.top + innerH * (1 - pct);
          return (
            <g key={pct}>
              <line
                x1={PADDING.left}
                y1={y}
                x2={CHART_WIDTH - PADDING.right}
                y2={y}
                stroke="var(--color-border)"
                strokeWidth="0.5"
              />
              <text
                x={PADDING.left - 8}
                y={y + 4}
                textAnchor="end"
                fill="var(--color-text-tertiary)"
                fontSize="10"
              >
                {Math.round(maxVal * pct)}
              </text>
            </g>
          );
        })}

        {/* 理想線（破線） */}
        <path
          d={idealPath}
          fill="none"
          stroke="var(--color-text-tertiary)"
          strokeWidth="1.5"
          strokeDasharray="6 3"
          opacity="0.6"
        />

        {/* スコープ線（階段状・破線） */}
        <path
          d={scopePath}
          fill="none"
          stroke="var(--color-warning)"
          strokeWidth="1.5"
          strokeDasharray="4 4"
          opacity="0.7"
        />

        {/* 実績線 */}
        {actualPath && (
          <>
            {/* 実績エリア（塗りつぶし） */}
            <path
              d={`${actualPath} L${toX(actualPoints.length - 1)},${toY(0)} L${toX(0)},${toY(0)} Z`}
              fill={isDelayed ? 'rgba(255, 77, 79, 0.08)' : 'rgba(99, 102, 241, 0.08)'}
            />
            {/* 実績ライン */}
            <path
              d={actualPath}
              fill="none"
              stroke={isDelayed ? 'var(--color-error)' : 'var(--color-accent)'}
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
            {/* 最新ポイントのドット */}
            <circle
              cx={toX(actualPoints.length - 1)}
              cy={toY(lastActual!.actual)}
              r="4"
              fill={isDelayed ? 'var(--color-error)' : 'var(--color-accent)'}
            />
          </>
        )}

        {/* X軸ラベル */}
        {points.map((p, i) => {
          if (i % labelInterval !== 0 && i !== points.length - 1) return null;
          const dateLabel = p.date.slice(5); // MM-DD
          return (
            <text
              key={i}
              x={toX(i)}
              y={CHART_HEIGHT - 10}
              textAnchor="middle"
              fill="var(--color-text-tertiary)"
              fontSize="9"
              transform={`rotate(-30, ${toX(i)}, ${CHART_HEIGHT - 10})`}
            >
              {dateLabel}
            </text>
          );
        })}

        {/* 凡例 */}
        <g transform={`translate(${CHART_WIDTH - PADDING.right - 210}, ${PADDING.top})`}>
          <line x1="0" y1="4" x2="15" y2="4" stroke="var(--color-text-tertiary)" strokeWidth="1.5" strokeDasharray="6 3" opacity="0.6" />
          <text x="18" y="8" fill="var(--color-text-tertiary)" fontSize="9">理想</text>
          <line x1="50" y1="4" x2="65" y2="4" stroke="var(--color-warning)" strokeWidth="1.5" strokeDasharray="4 4" opacity="0.7" />
          <text x="68" y="8" fill="var(--color-text-tertiary)" fontSize="9">スコープ</text>
          <line x1="120" y1="4" x2="135" y2="4" stroke="var(--color-accent)" strokeWidth="2" />
          <text x="138" y="8" fill="var(--color-text-secondary)" fontSize="9">実績</text>
        </g>
      </svg>
      <p className="burndown-chart__hint burndown-chart__hint--caption">
        {t('cycle.burndownColorHint')}
      </p>
    </div>
  );
}
