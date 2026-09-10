/**
 * MentionList.tsx — TipTapの@メンション候補ポップアップ
 *
 * @tiptap/extension-mention の suggestion.render() から呼び出される。
 * 上下キー・Enterでの選択、マウスクリックでの選択の両方に対応する。
 */
import { forwardRef, useEffect, useImperativeHandle, useState } from 'react';

export interface MentionListItem {
  id: number;
  username: string;
  displayName: string;
  alias?: string | null;
}

export interface MentionListProps {
  items: MentionListItem[];
  command: (item: { id: string; label: string }) => void;
}

export interface MentionListRef {
  onKeyDown: (props: { event: KeyboardEvent }) => boolean;
}

export const MentionList = forwardRef<MentionListRef, MentionListProps>((props, ref) => {
  const [selectedIndex, setSelectedIndex] = useState(0);

  useEffect(() => {
    setSelectedIndex(0);
  }, [props.items]);

  const selectItem = (index: number) => {
    const item = props.items[index];
    if (item) {
      props.command({
        id: String(item.id),
        label: item.displayName || item.alias || item.username,
      });
    }
  };

  useImperativeHandle(ref, () => ({
    onKeyDown: ({ event }) => {
      if (event.key === 'ArrowUp') {
        setSelectedIndex((selectedIndex + props.items.length - 1) % props.items.length);
        return true;
      }
      if (event.key === 'ArrowDown') {
        setSelectedIndex((selectedIndex + 1) % props.items.length);
        return true;
      }
      if (event.key === 'Enter') {
        selectItem(selectedIndex);
        return true;
      }
      return false;
    },
  }));

  if (props.items.length === 0) {
    return null;
  }

  return (
    <div className="mention-list">
      {props.items.map((item, index) => (
        <div
          key={item.id}
          className={`mention-list__item ${index === selectedIndex ? 'mention-list__item--selected' : ''}`}
          onMouseDown={(e) => { e.preventDefault(); selectItem(index); }}
          onMouseEnter={() => setSelectedIndex(index)}
        >
          <strong>{item.displayName || item.username}</strong>
          {item.alias && <span className="mention-list__alias">@{item.alias}</span>}
          <span className="mention-list__username">{item.username}</span>
        </div>
      ))}
    </div>
  );
});

MentionList.displayName = 'MentionList';
