/**
 * emojiCatalog.ts — チケット説明欄リアクション用絵文字固定セット
 *
 * 固定 11 Unicode + shortcode マップ
 */

export interface EmojiItem {
  emoji: string;
  name: string;
  shortcodes: string[];
}

export const EMOJI_CATALOG: EmojiItem[] = [
  {
    emoji: '👍',
    name: 'Thumbs Up',
    shortcodes: [':thumbsup:', ':+1:'],
  },
  {
    emoji: '👀',
    name: 'Eyes',
    shortcodes: [':eyes:'],
  },
  {
    emoji: '👏',
    name: 'Clapping Hands',
    shortcodes: [':clap:'],
  },
  {
    emoji: '❤️',
    name: 'Red Heart',
    shortcodes: [':heart:'],
  },
  {
    emoji: '✅',
    name: 'Check Mark',
    shortcodes: [':white_check_mark:', ':check:'],
  },
  {
    emoji: '🙇',
    name: 'Bowing Person',
    shortcodes: [':bow:'],
  },
  {
    emoji: '🙏',
    name: 'Folded Hands',
    shortcodes: [':pray:'],
  },
  {
    emoji: '🔥',
    name: 'Fire',
    shortcodes: [':fire:'],
  },
  {
    emoji: '🎉',
    name: 'Partying Face',
    shortcodes: [':tada:'],
  },
  {
    emoji: '😇',
    name: 'Smiling Face with Halo',
    shortcodes: [':innocent:'],
  },
  {
    emoji: '🤔',
    name: 'Thinking Face',
    shortcodes: [':thinking:', ':thinking_face:'],
  },
];

// Shortcode → Emoji マップ
const SHORTCODE_MAP = new Map<string, string>();
EMOJI_CATALOG.forEach((item) => {
  item.shortcodes.forEach((sc) => {
    SHORTCODE_MAP.set(sc, item.emoji);
  });
});

/**
 * Shortcode から Emoji を取得（`:thumbsup:` → `👍`）
 */
export function resolveEmoji(shortcode: string): string | null {
  return SHORTCODE_MAP.get(shortcode) ?? null;
}

/**
 * Emoji から name を取得
 */
export function getEmojiName(emoji: string): string | null {
  return EMOJI_CATALOG.find((item) => item.emoji === emoji)?.name ?? null;
}

/**
 * 固定絵文字のリスト
 */
export function getFixedEmojis(): string[] {
  return EMOJI_CATALOG.map((item) => item.emoji);
}

/**
 * Shortcode フィルター（`:` で始まる入力にマッチする shortcode）
 * 例: `:thumb` → [`:thumbsup:`, `:+1:`]
 */
export function filterShortcodes(input: string): string[] {
  if (!input.startsWith(':')) return [];

  const prefix = input.toLowerCase();
  return Array.from(SHORTCODE_MAP.keys()).filter((sc) => sc.startsWith(prefix));
}
