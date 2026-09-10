import { create } from 'zustand';
import { apiClient } from '@/shared/api/client';

type PromptGenerationPhase = 'idle' | 'generating' | 'success' | 'error';

interface PromptGenerationState {
  isOpen: boolean;
  phase: PromptGenerationPhase;
  ticketId: number | null;
  ticketKey: string;
  ticketTitle: string;
  promptText: string;
  errorMessage: string;
  elapsedMs: number | null;
  model: string | null;
  abortController: AbortController | null;

  openAndGenerate: (ticket: { id: number; ticketKey: string; title: string }, timeoutSecs: number, model: string) => void;
  close: () => void;
  copyToClipboard: () => Promise<void>;
  setPhase: (phase: PromptGenerationPhase) => void;
  setPromptText: (text: string) => void;
  setErrorMessage: (msg: string) => void;
  setElapsedMs: (ms: number | null) => void;
  setAbortController: (controller: AbortController | null) => void;
}

export const usePromptGenerationStore = create<PromptGenerationState>((set, get) => ({
  isOpen: false,
  phase: 'idle',
  ticketId: null,
  ticketKey: '',
  ticketTitle: '',
  promptText: '',
  errorMessage: '',
  elapsedMs: null,
  model: null,
  abortController: null,

  openAndGenerate: (ticket, timeoutSecs, model) => {
    const state = get();
    if (state.phase === 'generating' && state.ticketId !== ticket.id) {
      set({
        isOpen: true,
        phase: 'error',
        ticketId: ticket.id,
        ticketKey: ticket.ticketKey,
        ticketTitle: ticket.title,
        promptText: '',
        errorMessage: `「${state.ticketKey}」のプロンプトを生成中です。完了後に再度お試しください。`,
      });
      return;
    }

    const abortController = new AbortController();
    set({
      isOpen: true,
      phase: 'generating',
      ticketId: ticket.id,
      ticketKey: ticket.ticketKey,
      ticketTitle: ticket.title,
      promptText: '',
      errorMessage: '',
      elapsedMs: null,
      model,
      abortController,
    });

    generatePrompt(ticket.id, timeoutSecs, abortController);
  },

  close: () => {
    const state = get();
    if (state.abortController) {
      state.abortController.abort();
    }
    set({
      isOpen: false,
      phase: 'idle',
      ticketId: null,
      ticketKey: '',
      ticketTitle: '',
      promptText: '',
      errorMessage: '',
      elapsedMs: null,
      model: null,
      abortController: null,
    });
  },

  copyToClipboard: async () => {
    const text = get().promptText;
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
    } catch (error) {
      console.error('Failed to copy:', error);
      throw error;
    }
  },

  setPhase: (phase) => set({ phase }),
  setPromptText: (text) => set({ promptText: text }),
  setErrorMessage: (msg) => set({ errorMessage: msg }),
  setElapsedMs: (ms) => set({ elapsedMs: ms }),
  setAbortController: (controller) => set({ abortController: controller }),
}));

async function generatePrompt(ticketId: number, timeoutSecs: number, abortController: AbortController) {
  const { setPhase, setPromptText, setErrorMessage, setElapsedMs } = usePromptGenerationStore.getState();
  const startTime = Date.now();
  const timeoutHandle = setTimeout(() => abortController.abort(), (timeoutSecs + 15) * 1000);

  try {
    const response = await apiClient.post<{ prompt_text?: string; error?: string }>(
      '/ai/generate-prompt-text/',
      { ticket_id: ticketId },
      {
        signal: abortController.signal,
        timeout: (timeoutSecs + 15) * 1000,
      },
    );

    const elapsed = Date.now() - startTime;
    setElapsedMs(elapsed);

    const data = response.data;
    if (data.prompt_text) {
      setPromptText(data.prompt_text);
      setPhase('success');
    } else {
      setErrorMessage(data.error ?? 'プロンプトの生成に失敗しました。');
      setPhase('error');
    }
  } catch (error) {
    const elapsed = Date.now() - startTime;
    setElapsedMs(elapsed);

    if (error instanceof Error && (error.name === 'AbortError' || error.name === 'CanceledError')) {
      return;
    }

    const axiosError = error as { response?: { status?: number; data?: { error?: string } }; code?: string; message?: string };
    if (axiosError.response?.status === 504) {
      setErrorMessage('リバースプロキシがタイムアウトしました（約120〜300秒）。設定のタイムアウトを確認してください。');
    } else if (axiosError.response?.data?.error) {
      setErrorMessage(axiosError.response.data.error);
    } else if (axiosError.code === 'ECONNABORTED') {
      setErrorMessage(`クライアント待機がタイムアウトしました（${timeoutSecs}秒）。`);
    } else {
      setErrorMessage('プロンプトの生成に失敗しました。');
    }
    setPhase('error');
  } finally {
    clearTimeout(timeoutHandle);
  }
}
