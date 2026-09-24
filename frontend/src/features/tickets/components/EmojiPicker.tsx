import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { filterShortcodes, resolveEmoji, getFixedEmojis } from '../utils/emojiCatalog';
import { useCustomEmojis } from '../hooks/useCustomEmojis';
import './EmojiPicker.css';

export interface EmojiSelection {
  emojiKind: 'unicode' | 'custom';
  emojiValue: string;
}

interface EmojiPickerProps {
  onSelect: (selection: EmojiSelection) => void;
  onClose: () => void;
  projectPrefix?: string | null;
  projectId?: number | null;
}

const ITEM_HEIGHT = 44;

interface FilteredItem {
  kind: 'unicode' | 'custom' | 'upload';
  value: string;
  imageUrl?: string;
}

/**
 * EmojiPicker — 自前グリッド仮想化、キーボード操作、shortcode フィルター + カスタム絵文字
 */
export function EmojiPicker({
  onSelect,
  onClose,
  projectPrefix,
  projectId,
}: EmojiPickerProps) {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const { emojis: customEmojis, uploadEmoji } = useCustomEmojis(
    projectPrefix ?? null,
    projectId ?? null,
  );

  const [filterText, setFilterText] = useState('');
  const [uploadError, setUploadError] = useState<string | null>(null);
  const [filteredItems, setFilteredItems] = useState<FilteredItem[]>(() =>
    getFixedEmojis().map((u) => ({ kind: 'unicode' as const, value: u })),
  );
  const [cols, setCols] = useState(6);
  const [activeIndex, setActiveIndex] = useState(0);
  const [visibleRange, setVisibleRange] = useState({ start: 0, end: 12 });
  const [scrollTop, setScrollTop] = useState(0);
  const [fileInputRef, setFileInputRef] = useState<HTMLInputElement | null>(null);

  useEffect(() => {
    const items: FilteredItem[] = [];

    if (filterText.startsWith(':')) {
      const matched = filterShortcodes(filterText);
      const emojis = matched
        .map((sc) => resolveEmoji(sc))
        .filter((e) => e !== null) as string[];
      items.push(...[...new Set(emojis)].map((u) => ({ kind: 'unicode' as const, value: u })));

      const customMatches = customEmojis.filter(
        (c: typeof customEmojis[number]) => c.slug.includes(filterText.slice(1)) || c.name.toLowerCase().includes(filterText.toLowerCase()),
      );
      items.push(
        ...customMatches.map((c: typeof customEmojis[number]) => ({
          kind: 'custom' as const,
          value: c.id,
          imageUrl: c.imageUrl,
        })),
      );
    } else {
      items.push(...getFixedEmojis().map((u) => ({ kind: 'unicode' as const, value: u })));
      items.push(
        ...customEmojis.map((c: typeof customEmojis[number]) => ({
          kind: 'custom' as const,
          value: c.id,
          imageUrl: c.imageUrl,
        })),
      );
    }

    if (projectPrefix && projectId) {
      items.push({ kind: 'upload' as const, value: '__upload__' });
    }

    setFilteredItems(items);
    setActiveIndex(0);
  }, [filterText, customEmojis, projectPrefix, projectId]);

  useEffect(() => {
    const observer = new ResizeObserver(() => {
      if (!containerRef.current) return;
      const width = containerRef.current.clientWidth;
      const itemSize = 44;
      const newCols = Math.max(1, Math.floor(width / itemSize));
      setCols(newCols);
    });

    if (containerRef.current) {
      observer.observe(containerRef.current);
    }

    return () => observer.disconnect();
  }, []);

  const updateVisibleRange = useCallback(() => {
    if (!gridRef.current) return;
    const st = gridRef.current.scrollTop;
    const ch = gridRef.current.clientHeight;
    const startRow = Math.max(0, Math.floor(st / ITEM_HEIGHT) - 2);
    const endRow = Math.ceil((st + ch) / ITEM_HEIGHT) + 2;
    setVisibleRange({
      start: startRow * cols,
      end: Math.min(filteredItems.length, endRow * cols),
    });
  }, [cols, filteredItems.length]);

  useEffect(() => {
    updateVisibleRange();
  }, [activeIndex, scrollTop, updateVisibleRange]);

  useEffect(() => {
    if (!gridRef.current) return;
    const activeRow = Math.floor(activeIndex / cols);
    const targetTop = activeRow * ITEM_HEIGHT;
    const containerHeight = gridRef.current.clientHeight;
    const currentScrollTop = gridRef.current.scrollTop;

    if (targetTop < currentScrollTop) {
      gridRef.current.scrollTop = targetTop;
    } else if (targetTop + ITEM_HEIGHT > currentScrollTop + containerHeight) {
      gridRef.current.scrollTop = targetTop + ITEM_HEIGHT - containerHeight;
    }
  }, [activeIndex, cols]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const item = filteredItems[activeIndex];
      if (item) {
        if (item.kind === 'upload') {
          fileInputRef?.click();
        } else {
          onSelect({ emojiKind: item.kind, emojiValue: item.value });
          onClose();
        }
      }
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      setActiveIndex((prev) => Math.max(0, prev - 1));
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      setActiveIndex((prev) => Math.min(filteredItems.length - 1, prev + 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActiveIndex((prev) => Math.max(0, prev - cols));
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActiveIndex((prev) => Math.min(filteredItems.length - 1, prev + cols));
    }
  };

  const handleScroll = () => {
    if (gridRef.current) {
      setScrollTop(gridRef.current.scrollTop);
    }
  };

  const visibleItems = filteredItems.slice(visibleRange.start, visibleRange.end);

  const handleFileSelect = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const input = e.currentTarget;
    const file = input.files?.[0];
    if (!file || !projectPrefix || !projectId) return;

    setUploadError(null);

    const validMimes = ['image/jpeg', 'image/png', 'image/gif'];
    if (!validMimes.includes(file.type)) {
      console.error('Invalid mime type for emoji upload:', file.type);
      setUploadError(t('reaction.emojiUploadInvalidMime'));
      input.value = '';
      return;
    }

    const baseName = file.name.replace(/\.[^.]+$/, '');
    let slug = baseName
      .toLowerCase()
      .replace(/[^a-z0-9_]/g, '_')
      .slice(0, 32);
    if (!slug) {
      slug = `emoji_${Date.now()}`;
    }
    const name = baseName || slug;

    try {
      const localId = await uploadEmoji(file, slug, name);
      onSelect({ emojiKind: 'custom', emojiValue: localId });
      onClose();
    } catch (err) {
      console.error('Emoji upload failed:', err);
      setUploadError(t('reaction.emojiUploadError'));
    } finally {
      input.value = '';
    }
  };

  return (
    <div
      className="emoji-picker"
      ref={containerRef}
      onKeyDown={handleKeyDown}
      role="combobox"
      aria-expanded="true"
      aria-controls="emoji-grid"
      aria-activedescendant={`emoji-${activeIndex}`}
    >
      <input type="file" style={{ display: 'none' }} ref={setFileInputRef} onChange={handleFileSelect} accept="image/jpeg,image/png,image/gif" />
      <input
        ref={inputRef}
        type="text"
        className="emoji-picker__input"
        placeholder={t('reaction.emojiPickerPlaceholder')}
        value={filterText}
        onChange={(e) => setFilterText(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            onClose();
          } else if (
            e.key === 'ArrowLeft' ||
            e.key === 'ArrowRight' ||
            e.key === 'ArrowUp' ||
            e.key === 'ArrowDown' ||
            e.key === 'Enter'
          ) {
            handleKeyDown(e);
          }
        }}
        autoFocus
      />

      <div
        id="emoji-grid"
        ref={gridRef}
        className="emoji-picker__grid"
        role="listbox"
        onScroll={handleScroll}
        style={
          {
            '--cols': cols,
            '--item-height': `${ITEM_HEIGHT}px`,
          } as React.CSSProperties
        }
      >
        {visibleRange.start > 0 && (
          <div
            style={{
              gridColumn: '1 / -1',
              height: `${Math.floor(visibleRange.start / cols) * ITEM_HEIGHT}px`,
            }}
          />
        )}

        {visibleItems.map((item, idx) => {
          const actualIndex = visibleRange.start + idx;
          const isActive = actualIndex === activeIndex;
          return (
            <button
              key={actualIndex}
              id={`emoji-${actualIndex}`}
              className={`emoji-picker__item ${isActive ? 'emoji-picker__item--active' : ''}`}
              role="option"
              aria-selected={isActive}
              onClick={() => {
                if (item.kind === 'upload') {
                  fileInputRef?.click();
                } else {
                  onSelect({ emojiKind: item.kind, emojiValue: item.value });
                  onClose();
                }
              }}
              onMouseEnter={() => setActiveIndex(actualIndex)}
              type="button"
            >
              {item.kind === 'unicode' ? (
                item.value
              ) : item.kind === 'custom' ? (
                <img src={item.imageUrl} alt={item.value} style={{ width: '100%', height: '100%' }} />
              ) : (
                '➕'
              )}
            </button>
          );
        })}

        {visibleRange.end < filteredItems.length && (
          <div
            style={{
              gridColumn: '1 / -1',
              height: `${Math.ceil((filteredItems.length - visibleRange.end) / cols) * ITEM_HEIGHT}px`,
            }}
          />
        )}
      </div>

      {filteredItems.length === 0 && (
        <div className="emoji-picker__empty">{t('reaction.emojiPickerNoMatch')}</div>
      )}

      {uploadError && <div className="emoji-picker__error">{uploadError}</div>}
    </div>
  );
}
