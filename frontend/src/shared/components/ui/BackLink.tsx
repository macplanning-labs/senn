/**
 * BackLink.tsx — 詳細画面から、親の一覧へ戻るリンク(全画面共通)
 *
 * 画面ごとに戻り方が違うと迷うため、次のルールをこの部品に集約する:
 * - 位置: 画面の左上(見出しの上、またはヘッダーの左端)
 * - 文言: 「← {戻り先の一覧名}」(例: ← プロジェクト一覧)
 * - 動作: ブラウザの履歴ではなく、固定の親一覧へ移動する(どこから来ても同じ場所に戻る)
 * 詳細画面(一覧の1件を開いた画面)と、一覧の子にあたる画面(ロードマップなど)に置く。
 * サイドバーから直接開く一覧の画面(戻り先が無い)には置かない。
 */

import { Link } from 'react-router-dom';
import './BackLink.css';

interface BackLinkProps {
  /** 戻り先(親の一覧)のパス */
  to: string;
  /** 戻り先の名前(翻訳済み。「←」は部品が付ける) */
  label: string;
  /** テスト・E2E 用の識別子 */
  testId?: string;
  /** 他の要素と横に並ぶヘッダーの中に置くとき、下の余白を付けない */
  inline?: boolean;
}

export function BackLink({ to, label, testId = 'back-link', inline = false }: BackLinkProps) {
  return (
    <Link
      to={to}
      className={`back-link${inline ? ' back-link--inline' : ''}`}
      data-testid={testId}
    >
      <span aria-hidden="true">← </span>
      {label}
    </Link>
  );
}
