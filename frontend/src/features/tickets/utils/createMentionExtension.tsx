/**
 * createMentionExtension.tsx — TipTapの@メンション拡張ファクトリ（コメント入力欄で共有）
 *
 * @tiptap/extension-mention をラップし、候補一覧(userOptionsRef)から絞り込んだ
 * MentionList をポップアップ表示する。renderTextで`@[表示名:ID]`形式に変換し、
 * バックエンドのfind_mentioned_user_idsがID直接参照で検出できるようにする
 * （丸括弧形式`@[label](id)`はMarkdownリンクとして誤解釈されるため使わない）。
 *
 * TicketDetailPanel.tsx（旧・埋め込みパネル）と TicketComments.tsx（統合詳細画面）の
 * 両方から利用し、実装を分岐させない。
 */
import type { MutableRefObject, ReactNode } from 'react';
import { ReactRenderer } from '@tiptap/react';
import TiptapMention from '@tiptap/extension-mention';
import tippy from 'tippy.js';
import type { Instance as TippyInstance, GetReferenceClientRect } from 'tippy.js';
import { MentionList } from '../components/MentionList';
import type { MentionListRef, MentionListItem } from '../components/MentionList';

export interface MentionCandidate {
  id: number;
  username: string;
  displayName: string;
  alias?: string | null;
}

export function createMentionExtension(userOptionsRef: MutableRefObject<MentionCandidate[]>) {
  return TiptapMention.extend({
    // 注意: `@[label](id)` のような丸括弧付き形式はMarkdownのリンク記法と
    // 完全に一致してしまい、ReactMarkdownが実際にリンクとしてパースしてしまう
    // （このメンション検出ロジックが動く前に消費される）ため、コロン区切りの
    // 単一角括弧形式にする（`[text]`単体はCommonMarkのリンクにはならない）。
    renderText({ node }) {
      return `@[${node.attrs.label ?? node.attrs.id}:${node.attrs.id}]`;
    },
  }).configure({
    HTMLAttributes: { class: 'mention-node' },
    suggestion: {
      items: ({ query }: { query: string }): MentionListItem[] => {
        const q = query.toLowerCase();
        return userOptionsRef.current
          .filter((u) =>
            q.length === 0
            || u.username.toLowerCase().startsWith(q)
            || (u.displayName ?? '').toLowerCase().includes(q)
            || (u.alias ?? '').toLowerCase().includes(q)
          )
          .slice(0, 10);
      },
      render: () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        let component: ReactRenderer<MentionListRef, any>;
        let popup: TippyInstance[];
        return {
          onStart: (props) => {
            component = new ReactRenderer(MentionList, { props, editor: props.editor });
            if (!props.clientRect) return;
            popup = tippy('body', {
              getReferenceClientRect: props.clientRect as GetReferenceClientRect,
              appendTo: () => document.body,
              content: component.element,
              showOnCreate: true,
              interactive: true,
              trigger: 'manual',
              placement: 'bottom-start',
            });
          },
          onUpdate: (props) => {
            component.updateProps(props);
            if (!props.clientRect) return;
            popup[0]?.setProps({ getReferenceClientRect: props.clientRect as GetReferenceClientRect });
          },
          onKeyDown: (props) => {
            if (props.event.key === 'Escape') {
              popup[0]?.hide();
              return true;
            }
            return component.ref?.onKeyDown(props) ?? false;
          },
          onExit: () => {
            popup[0]?.destroy();
            component.destroy();
          },
        };
      },
    },
  });
}

/** TipTapのMention拡張が出力する `@[表示名:ID]` 形式を検出する正規表現（表示用） */
export const MENTION_BRACKET_REGEX = /@\[[^:\]]*:(\d+)\]/g;

/**
 * コメント本文中の `@[表示名:ID]` を、IDから引き直した最新の表示名でハイライト表示する。
 * 投稿後にユーザーが表示名を変更していても常に最新の表示名で表示できる。
 */
export function renderCommentBodyWithMentions(
  rawText: string,
  candidates: MentionCandidate[],
): ReactNode[] {
  const text = rawText.replace(/＠/g, '@');
  const userById = new Map<number, MentionCandidate>();
  candidates.forEach((u) => userById.set(u.id, u));

  const bracketMatches: { start: number; end: number; userId: number }[] = [];
  let bm: RegExpExecArray | null;
  const regex = new RegExp(MENTION_BRACKET_REGEX);
  while ((bm = regex.exec(text)) !== null) {
    bracketMatches.push({ start: bm.index, end: bm.index + bm[0].length, userId: Number(bm[1]) });
  }

  if (bracketMatches.length === 0) {
    return [text];
  }

  const parts: ReactNode[] = [];
  let cursor = 0;
  bracketMatches.forEach((match, idx) => {
    if (match.start > cursor) {
      parts.push(text.substring(cursor, match.start));
    }
    const user = userById.get(match.userId);
    parts.push(
      <span key={`mention-${idx}`} style={{ color: '#f97316', fontWeight: 500 }}>
        {`@${user?.displayName || user?.alias || user?.username || 'ユーザー'}`}
      </span>,
    );
    cursor = match.end;
  });
  if (cursor < text.length) {
    parts.push(text.substring(cursor));
  }
  return parts;
}
