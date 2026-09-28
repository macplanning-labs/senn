import type { TaskDependency } from '@/shared/api/types';

export type RelationCreateKind = 'blocking' | 'blockedBy' | 'related';

export interface ClassifiedDependencies {
  blocking: TaskDependency[];
  blockedBy: TaskDependency[];
  related: TaskDependency[];
}

export function classifyDependencies(
  deps: TaskDependency[],
  currentTicketId: number,
): ClassifiedDependencies {
  const blocking: TaskDependency[] = [];
  const blockedBy: TaskDependency[] = [];
  const related: TaskDependency[] = [];

  for (const dep of deps) {
    if (dep.dependencyType === 'relates_to') {
      if (dep.fromTask === currentTicketId || dep.toTask === currentTicketId) {
        related.push(dep);
      }
      continue;
    }
    if (dep.dependencyType !== 'blocks') continue;
    if (dep.fromTask === currentTicketId) blocking.push(dep);
    if (dep.toTask === currentTicketId) blockedBy.push(dep);
  }

  return { blocking, blockedBy, related };
}

export function peerTicketKey(dep: TaskDependency, currentTicketId: number): string {
  return dep.fromTask === currentTicketId ? dep.toTaskKey : dep.fromTaskKey;
}

export function peerTicketTitle(dep: TaskDependency, currentTicketId: number): string {
  return dep.fromTask === currentTicketId ? dep.toTaskTitle : dep.fromTaskTitle;
}

export function buildCreateDependencyRequest(
  kind: RelationCreateKind,
  currentTicketKey: string,
  currentTicketId: number,
  other: { id: number; ticketKey: string },
): { postTicketKey: string; body: { to_task: number; dependency_type: string } } {
  switch (kind) {
    case 'blocking':
      return {
        postTicketKey: currentTicketKey,
        body: { to_task: other.id, dependency_type: 'blocks' },
      };
    case 'blockedBy':
      return {
        postTicketKey: other.ticketKey,
        body: { to_task: currentTicketId, dependency_type: 'blocks' },
      };
    case 'related':
      return {
        postTicketKey: currentTicketKey,
        body: { to_task: other.id, dependency_type: 'relates_to' },
      };
  }
}
