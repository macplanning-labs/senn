import { localUpdateTicket } from '@/shared/sync/ticketWrites';

type UserValue = { id: number; username: string; displayName: string };
type LabelValue = { id: number; name: string; color: string };
type CategoryValue = { id: number; name: string; color: string };
type MilestoneValue = { id: number; name: string; dueDate: string | null };
export type CycleMutationValue = number | null | { id: number; name: string };

function extractCycleId(value: CycleMutationValue): number | null {
  if (value === null) return null;
  if (typeof value === 'object') return value.id;
  return value;
}

/**
 * Helper to create a local mutation that uses localUpdateTicket.
 * Returns an object with mutate, mutateAsync, and isPending (false) to match the old interface.
 */
function useLocalMutation<T>(
  ticketKey: string,
  toApi: (value: T) => Record<string, unknown>,
  toRow: (value: T) => Record<string, unknown>,
) {
  const mutate = (value: T) => {
    void localUpdateTicket(ticketKey, toApi(value), toRow(value));
  };
  const mutateAsync = async (value: T) => {
    await localUpdateTicket(ticketKey, toApi(value), toRow(value));
  };
  return { mutate, mutateAsync, isPending: false };
}

export function useTicketPropertyMutations(ticketId: string) {
  const statusMutation = useLocalMutation(
    ticketId,
    (status: string) => ({ status }),
    (status: string) => ({ status }),
  );

  const priorityMutation = useLocalMutation(
    ticketId,
    (priority: string) => ({ priority }),
    (priority: string) => ({ priority }),
  );

  const storyPointsMutation = useLocalMutation(
    ticketId,
    (storyPoints: number | null) => ({ story_points: storyPoints }),
    (storyPoints: number | null) => ({ storyPoints }),
  );

  const startDateMutation = useLocalMutation(
    ticketId,
    (startDate: string | null) => ({ start_date: startDate }),
    (startDate: string | null) => ({ startDate }),
  );

  const dueDateMutation = useLocalMutation(
    ticketId,
    (dueDate: string | null) => ({ due_date: dueDate }),
    (dueDate: string | null) => ({ dueDate }),
  );

  const cycleMutation = useLocalMutation(
    ticketId,
    (cycle: CycleMutationValue) => ({ cycle: extractCycleId(cycle) }),
    // id だけ渡されたときはサイクル名が分からないので名前は触らない（送信後にサーバーの値で揃う）
    (cycle: CycleMutationValue) =>
      typeof cycle === 'number'
        ? { cycle }
        : { cycle: extractCycleId(cycle), cycleName: cycle === null ? null : cycle.name },
  );

  const categoryMutation = useLocalMutation(
    ticketId,
    (category: CategoryValue | null) => ({ category: category?.id ?? null }),
    (category: CategoryValue | null) => ({ category }),
  );

  const milestoneMutation = useLocalMutation(
    ticketId,
    (milestone: MilestoneValue | null) => ({ milestone: milestone?.id ?? null }),
    (milestone: MilestoneValue | null) => ({ milestone }),
  );

  const labelsMutation = useLocalMutation(
    ticketId,
    (labels: LabelValue[]) => ({ labels: labels.map((l) => l.id) }),
    (labels: LabelValue[]) => ({ labels }),
  );

  const assigneesMutation = useLocalMutation(
    ticketId,
    (assignees: UserValue[]) => ({ assignees: assignees.map((a) => a.id) }),
    (assignees: UserValue[]) => ({ assignees }),
  );

  const reviewersMutation = useLocalMutation(
    ticketId,
    (reviewers: UserValue[]) => ({ reviewers: reviewers.map((r) => r.id) }),
    (reviewers: UserValue[]) => ({ reviewers }),
  );

  const typeMutation = useLocalMutation(
    ticketId,
    (type: string) => ({ ticket_type: type }),
    (type: string) => ({ ticketType: type }),
  );

  return {
    status: statusMutation,
    type: typeMutation,
    priority: priorityMutation,
    storyPoints: storyPointsMutation,
    startDate: startDateMutation,
    dueDate: dueDateMutation,
    cycle: cycleMutation,
    category: categoryMutation,
    milestone: milestoneMutation,
    labels: labelsMutation,
    assignees: assigneesMutation,
    reviewers: reviewersMutation,
  };
}
