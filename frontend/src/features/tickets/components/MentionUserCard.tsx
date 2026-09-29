import { useTranslation } from 'react-i18next';
import type { TicketUserOption } from '../utils/ticketUserOptions';

interface MentionUserCardProps {
  userId: number;
  userOptions: TicketUserOption[];
  /** Display name left in the body text when the user isn't in the candidate list. */
  fallbackName?: string;
}

export const MentionUserCard = ({ userId, userOptions, fallbackName }: MentionUserCardProps) => {
  const { t } = useTranslation();
  const user = userOptions.find((u) => u.id === userId);
  const name = user?.displayName || user?.username || fallbackName || t('ticket.mention.unknownUser');
  // 頭文字が取れないとき(空文字)のプレースホルダは絵文字にする。
  // CJKレンジの中点「・」は i18n.hardcoded-japanese.test.ts に日本語として検出されるため使わない。
  const firstChar = name.charAt(0) || '?';

  return (
    <div className="mention-user-card">
      <div className="mention-user-card__content">
        <div className="mention-user-card__avatar">{firstChar}</div>
        <div className="mention-user-card__info">
          <div className="mention-user-card__name">
            {name}
          </div>
          {user && (
            <div className="mention-user-card__username">@{user.username}</div>
          )}
        </div>
      </div>
    </div>
  );
};
