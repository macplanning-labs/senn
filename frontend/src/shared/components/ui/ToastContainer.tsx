/**
 * ToastContainer.tsx — トースト通知UI
 *
 * 画面右下に通知をスタック表示。
 * 楽観的UIのロールバック・成功・エラーを視覚フィードバック。
 */

import { useToastStore } from '@/shared/stores/toastStore';
import type { Toast } from '@/shared/stores/toastStore';
import './ToastContainer.css';

function ToastItem({ toast, onDismiss }: { toast: Toast; onDismiss: () => void }) {
  const icons: Record<Toast['type'], string> = {
    success: '✓',
    error: '✕',
    info: 'ℹ',
  };

  return (
    <div className={`toast toast--${toast.type}`} role="alert">
      <span className="toast__icon">{icons[toast.type]}</span>
      <span className="toast__message">{toast.message}</span>
      <button
        className="toast__dismiss"
        onClick={onDismiss}
        aria-label="閉じる"
      >
        ×
      </button>
    </div>
  );
}

export function ToastContainer() {
  const { toasts, removeToast } = useToastStore();

  if (toasts.length === 0) return null;

  return (
    <div className="toast-container" aria-live="polite">
      {toasts.map((toast) => (
        <ToastItem
          key={toast.id}
          toast={toast}
          onDismiss={() => removeToast(toast.id)}
        />
      ))}
    </div>
  );
}
