/**
 * useAiPromptTemplates.ts - ユーザーのAIプロンプト個人設定管理フック
 *
 * GET/PUT /auth/me/ai-prompt-templates/
 * 形式: { common: string|null, cursor: string|null, claude: string|null }
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useToast } from '@/shared/stores/toastStore';
import { useTranslation } from 'react-i18next';

export interface AiPromptTemplates {
  common: string | null;
  cursor: string | null;
  claude: string | null;
}

const QUERY_KEY = ['ai-prompt-templates'];

export function useAiPromptTemplates() {
  const { t } = useTranslation();
  const toast = useToast();
  const queryClient = useQueryClient();

  const query = useQuery({
    queryKey: QUERY_KEY,
    queryFn: async () => {
      const { data } = await apiClient.get<AiPromptTemplates>('/auth/me/ai-prompt-templates/');
      return data;
    },
  });

  const mutation = useMutation({
    mutationFn: async (templates: AiPromptTemplates) => {
      const { data } = await apiClient.put<AiPromptTemplates>('/auth/me/ai-prompt-templates/', templates);
      return data;
    },
    onSuccess: (data) => {
      queryClient.setQueryData(QUERY_KEY, data);
      toast.success(t('settings.aiPromptTemplateSaved'));
    },
    onError: () => {
      toast.error(t('settings.aiPromptTemplateSaveFailed'));
    },
  });

  return {
    templates: query.data,
    isLoading: query.isLoading,
    isError: query.isError,
    save: mutation.mutate,
    isSaving: mutation.isPending,
  };
}
