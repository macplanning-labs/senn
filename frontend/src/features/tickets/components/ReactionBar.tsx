import { useCallback, useMemo, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { useTicketReactions } from '../hooks/useTicketReactions';
import { useCustomEmojis } from '../hooks/useCustomEmojis';
import { EmojiPicker, type EmojiSelection } from './EmojiPicker';
import { getEmojiName } from '../utils/emojiCatalog';
import { useAuth } from '../../../shared/hooks/useAuth';
import './ReactionBar.css';

interface ReactionBarProps {
  ticketKey: string;
  ticketId: number;
  projectPrefix?: string | null;
  projectId?: number | null;
  /** 追加ボタンの表示（未指定時は 😊） */
  addButtonContent?: ReactNode;
  className?: string;
}

/**
 * ReactionBar — 説明欄直下のリアクション表示・操作UI
 */
export function ReactionBar({
  ticketKey,
  ticketId,
  projectPrefix,
  projectId,
  addButtonContent = '😊',
  className,
}: ReactionBarProps) {
  const { t } = useTranslation();
  const { user } = useAuth();
  const { reactions, toggleReaction } = useTicketReactions(ticketKey, ticketId);
  const { emojis: customEmojis } = useCustomEmojis(projectPrefix ?? null, projectId ?? null);
  const [showPicker, setShowPicker] = useState(false);

  const reactionGroups = useMemo(() => {
    const groups = new Map<
      string,
      {
        emojiKind: 'unicode' | 'custom';
        emoji: string;
        imageUrl?: string;
        count: number;
        userIds: Set<number>;
      }
    >();

    for (const reaction of reactions) {
      const key = `${reaction.emojiKind}:${reaction.emojiValue}`;
      const customEmoji = customEmojis.find((c: typeof customEmojis[number]) => c.id === reaction.emojiValue);
      const group = groups.get(key) || {
        emojiKind: reaction.emojiKind,
        emoji: reaction.emojiValue,
        imageUrl: customEmoji?.imageUrl,
        count: 0,
        userIds: new Set<number>(),
      };
      group.count++;
      group.userIds.add(reaction.userId);
      groups.set(key, group);
    }

    return Array.from(groups.values()).sort(
      (a, b) => b.count - a.count || a.emoji.localeCompare(b.emoji),
    );
  }, [reactions, customEmojis]);

  const handleSelectEmoji = useCallback(
    async (selection: EmojiSelection) => {
      if (!user) return;
      try {
        await toggleReaction(user.id, selection.emojiKind, selection.emojiValue);
      } catch {
        // SyncQueue にエントリ済み — オンライン復帰時に再試行
      }
    },
    [user, toggleReaction],
  );

  if (!user) return null;

  const rootClassName = className ? `reaction-bar ${className}` : 'reaction-bar';

  return (
    <div className={rootClassName}>
      <div className="reaction-bar__list">
        {reactionGroups.map((group) => {
          const isUserReacted = group.userIds.has(user.id);
          const emojiName = group.emojiKind === 'unicode' ? getEmojiName(group.emoji) : group.emoji;
          const countLabel = t('reaction.reactionCountTooltip', { count: group.count });

          return (
            <button
              key={`${group.emojiKind}:${group.emoji}`}
              className={`reaction-bar__chip ${isUserReacted ? 'reaction-bar__chip--active' : ''}`}
              onClick={async () => {
                try {
                  await toggleReaction(user.id, group.emojiKind, group.emoji);
                } catch {
                  // エラーは無視
                }
              }}
              title={emojiName ? `${emojiName} (${countLabel})` : `${group.emoji} (${countLabel})`}
              type="button"
            >
              <span className="reaction-bar__emoji">
                {group.emojiKind === 'custom' && group.imageUrl ? (
                  <img src={group.imageUrl} alt={group.emoji} style={{ width: '1em', height: '1em' }} />
                ) : (
                  group.emoji
                )}
              </span>
              <span className="reaction-bar__count">{group.count}</span>
            </button>
          );
        })}
      </div>

      <button
        className="reaction-bar__add-btn"
        onClick={() => setShowPicker(!showPicker)}
        title={t('reaction.addButtonTitle')}
        type="button"
        aria-label={t('reaction.addButtonLabel')}
        data-testid="ticket-reaction-add"
      >
        {addButtonContent}
      </button>

      {showPicker && (
        <div className="reaction-bar__picker-wrapper">
          <EmojiPicker
            onSelect={(selection) => {
              void handleSelectEmoji(selection);
              setShowPicker(false);
            }}
            onClose={() => setShowPicker(false)}
            projectPrefix={projectPrefix}
            projectId={projectId}
          />
        </div>
      )}
    </div>
  );
}
