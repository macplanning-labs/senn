import { useCallback } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { apiClient } from '@/shared/api/client';
import { useToastStore } from '@/shared/stores/toastStore';
import type { ReferenceLink } from '../types/ticketDetailView';
import type { TicketExtras } from '@/shared/sync/repos/ticketRepo';

type TicketCache = TicketExtras & Record<string, unknown>;

export function useReferenceLinkActions(ticketKey: string) {
  const { t } = useTranslation();
  const { addToast } = useToastStore();
  const queryClient = useQueryClient();
  const ticketQueryKey = ['ticket', ticketKey] as const;

  const addMutation = useMutation({
    mutationFn: async (payload: { url: string; title: string }) => {
      const { data } = await apiClient.post<ReferenceLink>(`/tickets/${ticketKey}/links/`, {
        url: payload.url,
        title: payload.title || undefined,
      });
      return data;
    },
    onSuccess: (link) => {
      queryClient.setQueryData<TicketCache | undefined>(ticketQueryKey, (old) => {
        if (!old) return old;
        return { ...old, links: [...(old.links ?? []), link] };
      });
      addToast({ type: 'success', message: t('ticketDetail.linkAdded') });
    },
    onError: () => {
      addToast({ type: 'error', message: t('ticketDetail.errors.linkFailed') });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: async (linkId: number) => {
      await apiClient.delete(`/tickets/${ticketKey}/links/${linkId}/`);
      return linkId;
    },
    onSuccess: (linkId) => {
      queryClient.setQueryData<TicketCache | undefined>(ticketQueryKey, (old) => {
        if (!old) return old;
        const nextLinks = (old.links as ReferenceLink[] | undefined) ?? [];
        return { ...old, links: nextLinks.filter((l) => l.id !== linkId) };
      });
      addToast({ type: 'success', message: t('ticketDetail.linkRemoved') });
    },
    onError: () => {
      addToast({ type: 'error', message: t('ticketDetail.errors.linkFailed') });
    },
  });

  const handleAdd = useCallback(
    async (url: string, title: string) => {
      const trimmedUrl = url.trim();
      if (!trimmedUrl) return;
      await addMutation.mutateAsync({ url: trimmedUrl, title: title.trim() });
    },
    [addMutation],
  );

  const handleDelete = useCallback(
    async (linkId: number) => {
      if (!window.confirm(t('ticketDetail.confirmDeleteLink'))) return;
      await deleteMutation.mutateAsync(linkId);
    },
    [deleteMutation, t],
  );

  return {
    handleAdd,
    handleDelete,
    isAdding: addMutation.isPending,
    isDeleting: deleteMutation.isPending,
  };
}
