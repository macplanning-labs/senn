import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useToastStore } from '@/shared/stores/toastStore';
import {
  getRelationErrorKey,
  useCreateTicketDependency,
  useDeleteTicketDependency,
} from './useTicketDependencies';
import type { RelationCreateKind } from '../utils/classifyDependencies';

export function useTicketRelationActions(
  ticketKey: string,
  ticketId: number,
  projectId: number | null | undefined,
  teamId: number | undefined,
) {
  const { t } = useTranslation();
  const { addToast } = useToastStore();
  const createMutation = useCreateTicketDependency(ticketKey, ticketId, projectId, teamId);
  const deleteMutation = useDeleteTicketDependency(ticketKey, projectId, teamId);

  const handleCreate = useCallback(
    async (kind: RelationCreateKind, other: { id: number; ticketKey: string }) => {
      try {
        await createMutation.mutateAsync({ kind, other });
        addToast({ type: 'success', message: t('ticketDetail.relationAdded') });
      } catch (err) {
        const key = getRelationErrorKey(err);
        addToast({ type: 'error', message: t(`ticketDetail.errors.${key}`) });
        throw err;
      }
    },
    [addToast, createMutation, t],
  );

  const handleDelete = useCallback(
    async (depId: number) => {
      try {
        await deleteMutation.mutateAsync(depId);
        addToast({ type: 'success', message: t('ticketDetail.relationRemoved') });
      } catch {
        addToast({ type: 'error', message: t('ticketDetail.errors.relationFailed') });
      }
    },
    [addToast, deleteMutation, t],
  );

  return {
    handleCreate,
    handleDelete,
    isCreating: createMutation.isPending,
    isDeleting: deleteMutation.isPending,
  };
}
