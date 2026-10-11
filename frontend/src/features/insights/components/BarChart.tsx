/**
 * BarChart.tsx - 素の SVG の棒グラフ(外部ライブラリなし)
 *
 * - 積み上げ(stacked)と並べ(grouped)に対応
 * - 値が 0 や null のときも、枠(細い線)が見えるようにする
 * - 色は CSS 変数(テーマ対応)。色だけに頼らず、凡例に文字も出す
 * - 各棒の title がツールチップになる
 */

import './BarChart.css';

export type BarChartColor = 'human' | 'ai' | 'unassigned';

export interface BarChartSeries {
  key: string;
  label: string;
  color: BarChartColor;
}

export interface BarChartRow {
  label: string;
  values: Record<string, number | null>;
}

interface BarChartProps {
  ariaLabel: string;
  rows: BarChartRow[];
  series: BarChartSeries[];
  stacked?: boolean;
  /** 値の表示(軸・ツールチップ)。null は「データなし」の表示に使う */
  formatValue: (value: number | null) => string;
}

const WIDTH = 640;
const HEIGHT = 240;
const MARGIN = { top: 12, right: 8, bottom: 28, left: 64 };
const TICKS = 4;

export function BarChart({ ariaLabel, rows, series, stacked = false, formatValue }: BarChartProps) {
  const plotWidth = WIDTH - MARGIN.left - MARGIN.right;
  const plotHeight = HEIGHT - MARGIN.top - MARGIN.bottom;

  // 縦軸の上限: 積み上げは行の合計の最大、並べは値の最大
  const maxValue = Math.max(
    0,
    ...rows.map((row) =>
      stacked
        ? series.reduce((sum, s) => sum + Math.max(0, row.values[s.key] ?? 0), 0)
        : series.reduce((max, s) => Math.max(max, row.values[s.key] ?? 0), 0),
    ),
  );
  const axisMax = maxValue > 0 ? maxValue : 1;
  const yOf = (value: number) => MARGIN.top + plotHeight - (Math.max(0, value) / axisMax) * plotHeight;

  const slotWidth = rows.length > 0 ? plotWidth / rows.length : plotWidth;
  const barSlot = stacked ? slotWidth * 0.6 : (slotWidth * 0.8) / Math.max(1, series.length);
  const barWidth = Math.min(barSlot, 40);
  // 横軸のラベルは、多すぎるときは間引く
  const labelStep = Math.max(1, Math.ceil(rows.length / 12));

  const ticks = Array.from({ length: TICKS + 1 }, (_, i) => (axisMax * i) / TICKS);

  return (
    <figure className="insights-chart">
      <svg
        className="insights-chart__svg"
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="img"
        aria-label={ariaLabel}
        preserveAspectRatio="xMidYMid meet"
      >
        {/* 目盛りの線と値 */}
        {ticks.map((tick) => (
          <g key={tick}>
            <line
              className="insights-chart__grid"
              x1={MARGIN.left}
              x2={WIDTH - MARGIN.right}
              y1={yOf(tick)}
              y2={yOf(tick)}
            />
            <text className="insights-chart__tick" x={MARGIN.left - 6} y={yOf(tick) + 4} textAnchor="end">
              {formatValue(tick)}
            </text>
          </g>
        ))}

        {rows.map((row, rowIndex) => {
          const slotX = MARGIN.left + rowIndex * slotWidth;
          const groupX = slotX + (slotWidth - barWidth * (stacked ? 1 : series.length)) / 2;
          let stackBase = 0;
          return (
            <g key={`${row.label}-${rowIndex}`}>
              {series.map((s, seriesIndex) => {
                const raw = row.values[s.key] ?? null;
                const value = raw == null ? 0 : Math.max(0, raw);
                let x: number;
                let y: number;
                let height: number;
                if (stacked) {
                  x = groupX;
                  y = yOf(stackBase + value);
                  height = yOf(stackBase) - yOf(stackBase + value);
                  stackBase += value;
                } else {
                  x = groupX + seriesIndex * barWidth;
                  y = yOf(value);
                  height = yOf(0) - y;
                }
                return (
                  <rect
                    key={s.key}
                    className={`insights-chart__bar insights-chart__bar--${s.color}`}
                    style={{ fill: `var(--insights-${s.color})` }}
                    x={x}
                    y={y}
                    width={barWidth}
                    height={Math.max(0, height)}
                  >
                    <title>{`${row.label} ${s.label}: ${formatValue(raw)}`}</title>
                  </rect>
                );
              })}
              {rowIndex % labelStep === 0 && (
                <text
                  className="insights-chart__tick"
                  x={slotX + slotWidth / 2}
                  y={HEIGHT - 8}
                  textAnchor="middle"
                >
                  {row.label}
                </text>
              )}
            </g>
          );
        })}

        {/* 横軸の線 */}
        <line
          className="insights-chart__axis"
          x1={MARGIN.left}
          x2={WIDTH - MARGIN.right}
          y1={yOf(0)}
          y2={yOf(0)}
        />
      </svg>

      {/* 凡例(色だけに頼らず、文字でも出す) */}
      <figcaption className="insights-chart__legend">
        {series.map((s) => (
          <span key={s.key} className="insights-chart__legend-item">
            <span
              className={`insights-chart__swatch insights-chart__swatch--${s.color}`}
              style={{ background: `var(--insights-${s.color})` }}
              aria-hidden="true"
            />
            {s.label}
          </span>
        ))}
      </figcaption>
    </figure>
  );
}
