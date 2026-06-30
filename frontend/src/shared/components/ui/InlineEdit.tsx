/**
 * InlineEdit.tsx — 汎用インライン編集コンポーネント
 *
 * ダブルクリックで編集モードに入り、Enter で確定、Escape でキャンセル。
 * type: 'text' | 'select' | 'date' に対応。
 */

import { useState, useRef, useEffect, useCallback } from 'react';
import './InlineEdit.css';

interface InlineEditTextProps {
  type: 'text';
  value: string;
  onSave: (value: string) => void | Promise<void>;
  placeholder?: string;
  className?: string;
}

interface InlineEditSelectProps {
  type: 'select';
  value: string;
  options: { value: string; label: string }[];
  onSave: (value: string) => void | Promise<void>;
  className?: string;
}

interface InlineEditDateProps {
  type: 'date';
  value: string;
  onSave: (value: string) => void | Promise<void>;
  className?: string;
}

type InlineEditProps = InlineEditTextProps | InlineEditSelectProps | InlineEditDateProps;

export function InlineEdit(props: InlineEditProps) {
  const { type, value, onSave, className = '' } = props;
  const [editing, setEditing] = useState(false);
  const [editValue, setEditValue] = useState(value);
  const [saving, setSaving] = useState(false);
  const inputRef = useRef<HTMLInputElement | HTMLSelectElement>(null);

  // 値が外から変更されたら同期
  useEffect(() => {
    setEditValue(value);
  }, [value]);

  // 編集モードに入ったらフォーカス
  useEffect(() => {
    if (editing && inputRef.current) {
      inputRef.current.focus();
      if (inputRef.current instanceof HTMLInputElement) {
        inputRef.current.select();
      }
    }
  }, [editing]);

  const handleSave = useCallback(async () => {
    if (editValue === value) {
      setEditing(false);
      return;
    }
    try {
      setSaving(true);
      await onSave(editValue);
    } finally {
      setSaving(false);
      setEditing(false);
    }
  }, [editValue, value, onSave]);

  const handleCancel = useCallback(() => {
    setEditValue(value);
    setEditing(false);
  }, [value]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        void handleSave();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        handleCancel();
      }
    },
    [handleSave, handleCancel],
  );

  // セレクトの場合はクリックで即編集
  if (type === 'select' && 'options' in props) {
    return (
      <select
        ref={inputRef as React.Ref<HTMLSelectElement>}
        className={`inline-edit inline-edit--select ${className}`}
        value={value}
        onChange={(e) => {
          e.stopPropagation();
          void onSave(e.target.value);
        }}
        onClick={(e) => e.stopPropagation()}
        disabled={saving}
      >
        {props.options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    );
  }

  // 表示モード
  if (!editing) {
    return (
      <span
        className={`inline-edit inline-edit--display ${className}`}
        onDoubleClick={(e) => {
          e.stopPropagation();
          setEditing(true);
        }}
        title="Double-click to edit"
      >
        {value || <span className="inline-edit__placeholder">{('placeholder' in props && props.placeholder) || '—'}</span>}
      </span>
    );
  }

  // 編集モード
  return (
    <input
      ref={inputRef as React.Ref<HTMLInputElement>}
      className={`inline-edit inline-edit--editing ${className}`}
      type={type === 'date' ? 'date' : 'text'}
      value={editValue}
      onChange={(e) => setEditValue(e.target.value)}
      onKeyDown={handleKeyDown}
      onBlur={() => void handleSave()}
      onClick={(e) => e.stopPropagation()}
      disabled={saving}
    />
  );
}
