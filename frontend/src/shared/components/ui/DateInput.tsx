import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import './DateInput.css';

export interface DateInputProps {
  id?: string;
  value: string | null;
  onChange: (value: string | null) => void;
  placeholder?: string;
  testId?: string;
  disabled?: boolean;
  className?: string;
  /** false のときクリアを出さない（必須の日付） */
  allowClear?: boolean;
  /** 増やすとカレンダーを開く（ショートカット用） */
  openSignal?: number;
  variant?: 'field' | 'inline';
}

/** Local calendar day as YYYY-MM-DD. toISOString is UTC and shifts the day in JST. */
export function toLocalIsoDate(date: Date): string {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, '0');
  const d = String(date.getDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

function parseLocalIsoDate(iso: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!match) return null;
  const y = Number(match[1]);
  const m = Number(match[2]);
  const d = Number(match[3]);
  const date = new Date(y, m - 1, d);
  if (date.getFullYear() !== y || date.getMonth() !== m - 1 || date.getDate() !== d) return null;
  return date;
}

function MonthCalendar({
  value,
  onChange,
}: {
  value: string | null;
  onChange: (isoDate: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const locale = i18n.language.startsWith('ja') ? 'ja-JP' : 'en-US';
  const today = new Date();
  const parsedValue = value ? parseLocalIsoDate(value) : null;
  const [displayMonth, setDisplayMonth] = useState(() => parsedValue ?? today);
  const [selectedDate, setSelectedDate] = useState<Date | null>(parsedValue);

  useEffect(() => {
    const parsed = value ? parseLocalIsoDate(value) : null;
    setSelectedDate(parsed);
    if (parsed) setDisplayMonth(new Date(parsed.getFullYear(), parsed.getMonth(), 1));
  }, [value]);

  const year = displayMonth.getFullYear();
  const monthLabel = displayMonth.toLocaleDateString(locale, { year: 'numeric', month: 'long' });
  const weekdayFmt = new Intl.DateTimeFormat(locale, { weekday: 'short' });
  const weekdays = Array.from({ length: 7 }, (_, i) => weekdayFmt.format(new Date(2023, 0, 1 + i)));
  const firstWeekday = new Date(year, displayMonth.getMonth(), 1).getDay();
  const daysInMonth = new Date(year, displayMonth.getMonth() + 1, 0).getDate();
  const days: Array<number | null> = [];
  for (let i = 0; i < firstWeekday; i++) days.push(null);
  for (let i = 1; i <= daysInMonth; i++) days.push(i);

  const isToday = (day: number) =>
    day === today.getDate() &&
    displayMonth.getMonth() === today.getMonth() &&
    displayMonth.getFullYear() === today.getFullYear();

  const isSelected = (day: number) =>
    selectedDate != null &&
    day === selectedDate.getDate() &&
    displayMonth.getMonth() === selectedDate.getMonth() &&
    displayMonth.getFullYear() === selectedDate.getFullYear();

  return (
    <div className="month-calendar">
      <div className="month-calendar__header">
        <button
          type="button"
          className="month-calendar__nav-btn"
          onClick={() => setDisplayMonth((prev) => new Date(prev.getFullYear(), prev.getMonth() - 1, 1))}
          aria-label={t('common.calendarPrev', 'Previous month')}
        >
          ‹
        </button>
        <div className="month-calendar__month-year">{monthLabel}</div>
        <button
          type="button"
          className="month-calendar__nav-btn"
          onClick={() => setDisplayMonth((prev) => new Date(prev.getFullYear(), prev.getMonth() + 1, 1))}
          aria-label={t('common.calendarNext', 'Next month')}
        >
          ›
        </button>
      </div>
      <div className="month-calendar__weekdays">
        {weekdays.map((label) => (
          <div key={label} className="month-calendar__weekday">{label}</div>
        ))}
      </div>
      <div className="month-calendar__grid">
        {days.map((day, idx) =>
          day === null ? (
            <div key={`empty-${idx}`} className="month-calendar__day--empty" />
          ) : (
            <button
              key={day}
              type="button"
              className={`month-calendar__day${isSelected(day) ? ' month-calendar__day--selected' : ''}${isToday(day) ? ' month-calendar__day--today' : ''}`}
              onClick={() => {
                const selected = new Date(displayMonth.getFullYear(), displayMonth.getMonth(), day);
                setSelectedDate(selected);
                onChange(toLocalIsoDate(selected));
              }}
            >
              {day}
            </button>
          ),
        )}
      </div>
    </div>
  );
}

export function DateInput({
  id,
  value,
  onChange,
  placeholder,
  testId,
  disabled = false,
  className,
  allowClear = true,
  openSignal = 0,
  variant = 'field',
}: DateInputProps) {
  const { t } = useTranslation();
  const iso = value ? parseLocalIsoDate(value.slice(0, 10)) : null;
  const isoText = iso ? toLocalIsoDate(iso) : null;
  const [open, setOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (openSignal > 0) setOpen(true);
  }, [openSignal]);

  useEffect(() => {
    if (!open) return;
    const onDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (menuRef.current?.contains(target)) return;
      if (buttonRef.current?.contains(target)) return;
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  const rect = open ? buttonRef.current?.getBoundingClientRect() : null;
  let menuStyle: CSSProperties | undefined;
  if (rect) {
    const menuHeight = 380;
    const top =
      rect.bottom + 8 + menuHeight > window.innerHeight && rect.top - menuHeight - 8 >= 8
        ? rect.top - menuHeight - 8
        : rect.bottom + 8;
    const left = Math.min(Math.max(8, rect.left), window.innerWidth - 320 - 8);
    menuStyle = { top, left };
  }

  const placeholderText = placeholder ?? t('common.noDate', 'No date');

  return (
    <div className={`date-input${variant === 'inline' ? ' date-input--inline' : ''}`}>
      <button
        type="button"
        id={id}
        ref={buttonRef}
        className={`date-input__button${className ? ` ${className}` : ''}`}
        disabled={disabled}
        data-testid={testId}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        {isoText ?? placeholderText}
      </button>
      {open && menuStyle && createPortal(
        <div ref={menuRef} className="date-input__popover" style={menuStyle} role="dialog">
          <MonthCalendar
            value={isoText}
            onChange={(next) => {
              onChange(next);
              setOpen(false);
            }}
          />
          {allowClear && (
            <div className="date-input__footer">
              <button
                type="button"
                className="date-input__clear"
                onClick={() => {
                  onChange(null);
                  setOpen(false);
                }}
              >
                {t('common.clear', 'Clear')}
              </button>
            </div>
          )}
        </div>,
        document.body,
      )}
    </div>
  );
}
