import { runCycle } from '@/shared/sync/syncEngine';

/** BG の ai_prompt キャッシュ生成完了後に sync pull する待ち時間 */
const AI_PROMPT_CACHE_SYNC_DELAY_MS = 8_000;

/** コメント追加後、キャッシュ済み aiPrompt をローカル DB に反映する */
export function scheduleAiPromptCacheSync(): void {
  window.setTimeout(() => {
    void runCycle('ai-prompt-cache');
  }, AI_PROMPT_CACHE_SYNC_DELAY_MS);
}
