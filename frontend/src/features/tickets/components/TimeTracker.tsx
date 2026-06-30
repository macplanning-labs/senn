/**
 * TimeTracker.tsx — タイムトラッカーUI
 *
 * 2つのモード:
 * 1. タイマーモード: 開始→停止でリアルタイム計測
 * 2. 手動入力モード: 時間(h)と分(m)を直接入力
 *
 * チケット詳細パネルに組み込む。
 */
import { useState, useEffect, useRef } from 'react';
import { useCreateTimeEntry, useTimeEntries, useDeleteTimeEntry } from '../hooks/useTimeEntries';
import type { TimeEntry } from '@/shared/api/types';
import './TimeTracker.css';

function formatDuration(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `${m}m`;
  return `${h}h ${m}m`;
}

function formatElapsed(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

export function TimeTracker({ ticketId }: { ticketId: number }) {
  const { data: entries = [] } = useTimeEntries(ticketId);
  const createMutation = useCreateTimeEntry();
  const deleteMutation = useDeleteTimeEntry();

  // タイマー状態
  const [isRunning, setIsRunning] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [startTime, setStartTime] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // 手動入力状態
  const [showManual, setShowManual] = useState(false);
  const [manualHours, setManualHours] = useState('');
  const [manualMinutes, setManualMinutes] = useState('');
  const [description, setDescription] = useState('');

  // 合計時間
  const totalMinutes = entries.reduce((sum, e) => sum + e.durationMinutes, 0);

  // タイマー
  useEffect(() => {
    if (isRunning) {
      timerRef.current = setInterval(() => {
        setElapsed(prev => prev + 1);
      }, 1000);
    }
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [isRunning]);

  const handleStart = () => {
    setStartTime(new Date().toISOString());
    setElapsed(0);
    setIsRunning(true);
  };

  const handleStop = () => {
    setIsRunning(false);
    if (timerRef.current) clearInterval(timerRef.current);
    const endTime = new Date().toISOString();
    createMutation.mutate({
      ticket: ticketId,
      start_time: startTime!,
      end_time: endTime,
      description,
    }, {
      onSuccess: () => {
        setStartTime(null);
        setElapsed(0);
        setDescription('');
      },
    });
  };

  const handleManualSubmit = () => {
    const h = parseInt(manualHours || '0');
    const m = parseInt(manualMinutes || '0');
    const total = h * 60 + m;
    if (total <= 0) return;
    createMutation.mutate({
      ticket: ticketId,
      duration_minutes: total,
      description,
    }, {
      onSuccess: () => {
        setManualHours('');
        setManualMinutes('');
        setDescription('');
        setShowManual(false);
      },
    });
  };

  return (
    <div className="time-tracker">
      <div className="time-tracker__header">
        <h4 className="time-tracker__title">⏱ 時間記録</h4>
        <span className="time-tracker__total">{formatDuration(totalMinutes)}</span>
      </div>

      {/* タイマー */}
      <div className="time-tracker__timer">
        {isRunning ? (
          <>
            <span className="time-tracker__elapsed">{formatElapsed(elapsed)}</span>
            <button className="time-tracker__btn time-tracker__btn--stop" onClick={handleStop}>
              ■ 停止
            </button>
          </>
        ) : (
          <>
            <button className="time-tracker__btn time-tracker__btn--start" onClick={handleStart}>
              ▶ 開始
            </button>
            <button
              className="time-tracker__btn time-tracker__btn--manual"
              onClick={() => setShowManual(!showManual)}
            >
              ✏️ 手動
            </button>
          </>
        )}
      </div>

      {/* 作業内容（タイマー中 or 手動入力中に表示） */}
      {(isRunning || showManual) && (
        <input
          className="time-tracker__input"
          placeholder="作業内容（任意）"
          value={description}
          onChange={e => setDescription(e.target.value)}
        />
      )}

      {/* 手動入力フォーム */}
      {showManual && (
        <div className="time-tracker__manual">
          <input
            type="number"
            className="time-tracker__input time-tracker__input--small"
            placeholder="0"
            value={manualHours}
            onChange={e => setManualHours(e.target.value)}
            min="0"
          />
          <span className="time-tracker__unit">h</span>
          <input
            type="number"
            className="time-tracker__input time-tracker__input--small"
            placeholder="0"
            value={manualMinutes}
            onChange={e => setManualMinutes(e.target.value)}
            min="0"
            max="59"
          />
          <span className="time-tracker__unit">m</span>
          <button
            className="time-tracker__btn time-tracker__btn--submit"
            onClick={handleManualSubmit}
          >
            記録
          </button>
        </div>
      )}

      {/* 記録一覧 */}
      {entries.length > 0 && (
        <div className="time-tracker__entries">
          {entries.slice(0, 5).map((entry) => (
            <TimeEntryRow
              key={entry.id}
              entry={entry}
              onDelete={() => deleteMutation.mutate({ id: entry.id, ticketId })}
            />
          ))}
          {entries.length > 5 && (
            <span className="time-tracker__more">
              他 {entries.length - 5} 件
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function TimeEntryRow({
  entry,
  onDelete,
}: {
  entry: TimeEntry;
  onDelete: () => void;
}) {
  const date = new Date(entry.createdAt).toLocaleDateString('ja-JP', {
    month: 'short',
    day: 'numeric',
  });

  return (
    <div className="time-entry-row">
      <span className="time-entry-row__user">
        {entry.user.firstName || entry.user.username}
      </span>
      <span className="time-entry-row__duration">
        {formatDuration(entry.durationMinutes)}
      </span>
      {entry.description && (
        <span className="time-entry-row__desc">{entry.description}</span>
      )}
      <span className="time-entry-row__date">{date}</span>
      <button
        className="time-entry-row__delete"
        onClick={onDelete}
        title="削除"
      >
        ×
      </button>
    </div>
  );
}
