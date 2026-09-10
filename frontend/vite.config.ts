/// <reference types="vitest/config" />

/// vite.config.ts — WIP フロントエンド ビルド設定
///
/// パスエイリアス (@/) と開発用APIプロキシを設定。

import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    port: 5173,
    // Rust API へのプロキシ（開発時のCORS回避、Django撤去後はport 8151）
    proxy: {
      '/api': {
        target: 'http://localhost:8151',
        changeOrigin: true,
      },
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
})

