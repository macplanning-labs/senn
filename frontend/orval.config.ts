/**
 * orval.config.ts — OpenAPI → axios + React Query 自動生成設定
 *
 * `npx orval` で実行すると、openapi.json から以下を自動生成:
 *   - axios リクエスト関数
 *   - TanStack Query hooks (useQuery / useMutation)
 *   - TypeScript 型定義
 *
 * 生成先: src/generated/api/
 */
import { defineConfig } from 'orval';

export default defineConfig({
  senn: {
    input: {
      target: './openapi.json',
    },
    output: {
      target: './src/generated/api/endpoints.ts',
      schemas: './src/generated/api/model',
      client: 'react-query',
      mode: 'tags-split',
      httpClient: 'axios',
      override: {
        mutator: {
          path: './src/shared/api/client.ts',
          name: 'apiClient',
        },
        query: {
          useQuery: true,
          useMutation: true,
        },
      },
    },
  },
});
