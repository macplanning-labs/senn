/**
 * VelocityChart.tsx — ベロシティ棒グラフ（SVGベース）
 *
 * ライブラリ不使用でバンドルサイズを抑制。
 * 完了ポイントの棒グラフ + 平均ラインを表示。
 */
import { useVelocity } from '../hooks/useCycles';
import './VelocityChart.css';

const CHART_WIDTH = 600;
const CHART_HEIGHT = 200;
const BAR_GAP = 12;
const PADDING = { top: 20, right: 20, bottom: 40, left: 50 };

export function VelocityChart({ projectId }: { projectId: number }) {
  const { data: velocityData = [] } = useVelocity(projectId);

  if (velocityData.length === 0) return null;

  const innerW = CHART_WIDTH - PADDING.left - PADDING.right;
  const innerH = CHART_HEIGHT - PADDING.top - PADDING.bottom;

  const maxVal = Math.max(...velocityData.map(d => d.completedPoints), 1);
  const barWidth = (innerW - BAR_GAP * (velocityData.length - 1)) / velocityData.length;

  const avgPoints = velocityData.reduce((sum, d) => sum + d.completedPoints, 0) / velocityData.length;
  const avgY = PADDING.top + innerH - (avgPoints / maxVal) * innerH;

  return (
    <div className="velocity-chart">
      <h3 className="velocity-chart__title">ベロシティ</h3>
      <svg
        viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`}
        className="velocity-chart__svg"
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

        {/* 棒グラフ */}
        {velocityData.map((d, i) => {
          const barH = (d.completedPoints / maxVal) * innerH;
          const x = PADDING.left + i * (barWidth + BAR_GAP);
          const y = PADDING.top + innerH - barH;

          return (
            <g key={d.cycleNumber} className="velocity-chart__bar-group">
              {/* 棒 */}
              <rect
                x={x}
                y={y}
                width={barWidth}
                height={barH}
                rx="4"
                fill="url(#velocity-gradient)"
                className="velocity-chart__bar"
              />
              {/* 棒の上のポイント数 */}
              <text
                x={x + barWidth / 2}
                y={y - 6}
                textAnchor="middle"
                fill="var(--color-text-secondary)"
                fontSize="11"
                fontWeight="600"
              >
                {d.completedPoints}
              </text>
              {/* X軸ラベル */}
              <text
                x={x + barWidth / 2}
                y={CHART_HEIGHT - 10}
                textAnchor="middle"
                fill="var(--color-text-tertiary)"
                fontSize="10"
              >
                #{d.cycleNumber}
              </text>
              {/* キャリーオーバーインジケーター */}
              {d.carryOver > 0 && (
                <rect
                  x={x}
                  y={y + barH - (d.carryOver / maxVal) * innerH}
                  width={barWidth}
                  height={Math.max((d.carryOver / maxVal) * innerH, 2)}
                  rx="4"
                  fill="var(--color-warning)"
                  opacity="0.6"
                />
              )}
              {/* ツールチップ領域 */}
              <title>
                {d.cycleName}: {d.completedPoints}pt 完了, {d.carryOver}件 持越
              </title>
            </g>
          );
        })}

        {/* 平均ライン */}
        <line
          x1={PADDING.left}
          y1={avgY}
          x2={CHART_WIDTH - PADDING.right}
          y2={avgY}
          stroke="var(--color-accent)"
          strokeWidth="1.5"
          strokeDasharray="6 3"
          opacity="0.7"
        />
        <text
          x={CHART_WIDTH - PADDING.right + 4}
          y={avgY + 4}
          fill="var(--color-accent)"
          fontSize="10"
          fontWeight="600"
        >
          avg {Math.round(avgPoints)}
        </text>

        {/* グラデーション定義 */}
        <defs>
          <linearGradient id="velocity-gradient" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#6366f1" />
            <stop offset="100%" stopColor="#14b8a6" />
          </linearGradient>
        </defs>
      </svg>
    </div>
  );
}
