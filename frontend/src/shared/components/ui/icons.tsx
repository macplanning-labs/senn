/**
 * icons.tsx — アイコンだけのボタンで使う共通アイコン
 *
 * ルール:
 * - アイコンだけのボタン（文字ラベルなし）は、文字の「+」「···」「⋯」ではなくこの SVG を使う。
 *   文字はフォントによって高さ・太さが変わり、隣のアイコンとそろわないため
 * - 文字ラベル付きのボタン（「+ チケット作成」など）は、先頭の「+」を文字のままにする
 * - 絵文字は、AI や状態など気持ちを伝えるボタンに使う（構造を示す記号には使わない）
 * - 読み上げ用に、ボタン側へ aria-label（または title）を必ず付ける
 */

interface IconProps {
  /** 一辺の大きさ（px） */
  size?: number;
}

/** 追加（＋） */
export function IconPlus({ size = 14 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      aria-hidden="true"
      className="ui-icon"
    >
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

/** その他の操作（横三点 …） */
export function IconMoreHorizontal({ size = 14 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
      className="ui-icon"
    >
      <circle cx="5" cy="12" r="2" />
      <circle cx="12" cy="12" r="2" />
      <circle cx="19" cy="12" r="2" />
    </svg>
  );
}
