/**
 * main.tsx — WIP アプリケーションエントリポイント
 *
 * i18n初期化 + テーマ適用 + React DOMレンダリング。
 */

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import './i18n';
import './styles/index.css';

// 保存済みのテーマを適用
const savedTheme = localStorage.getItem('wip-ui-preferences');
if (savedTheme) {
  try {
    const parsed = JSON.parse(savedTheme) as { state?: { theme?: string } };
    const theme = parsed.state?.theme ?? 'dark';
    document.documentElement.setAttribute('data-theme', theme);
  } catch {
    document.documentElement.setAttribute('data-theme', 'dark');
  }
} else {
  document.documentElement.setAttribute('data-theme', 'dark');
}

const rootEl = document.getElementById('root');
if (!rootEl) throw new Error('Root element not found');

createRoot(rootEl).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
