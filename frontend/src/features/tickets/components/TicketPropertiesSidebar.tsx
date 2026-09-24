import { useCallback, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { apiClient } from '@/shared/api/client';
import { useProject } from '@/shared/hooks/useProject';
import { useToastStore } from '@/shared/stores/toastStore';
import { useWorkflowStatuses } from '@/features/settings/hooks/useWorkflowStatuses';
import type { TicketDetailView } from '../types/ticketDetailView';
import { fetchTicketUserOptions, ticketUserOptionsEnabled } from '../utils/ticketUserOptions';
import { useTicketPropertyMutations } from '../hooks/useTicketPropertyMutations';
import { useTicketDetailHotkeys, type TicketHotkeyAction } from '../hooks/useTicketDetailHotkeys';
import {
  TicketFieldPicker,
  type TicketFieldPickerOption,
  type TicketPickerField,
} from './TicketFieldPicker';

const PRIORITIES = ['urgent', 'high', 'medium', 'low'] as const;
const ESTIMATES = [1, 2, 3, 5, 8, 13, 21] as const;

/** Properties / Others section labels are English-fixed for keyboard shortcut UX (design §4). */
const PICKER_FIELD_TITLES: Record<TicketPickerField, string> = {
  status: 'Status',
  assignee: 'Assignee',
  priority: 'Priority',
  cycle: 'Cycle',
  project: 'Project',
  labels: 'Labels',
  estimate: 'Estimate',
  dueDate: 'Due Date',
  startDate: 'Start date',
  category: 'Category',
  milestone: 'Milestone',
  reviewer: 'Reviewer',
};

interface TicketPropertiesSidebarProps {
  ticket: TicketDetailView;
  ticketId: string;
  onFocusTitle: () => void;
  onFocusDescription: () => void;
}

function formatDate(dateStr: string | null | undefined): string {
  if (!dateStr) return '—';
  return new Date(dateStr).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

export function TicketPropertiesSidebar({
  ticket,
  ticketId,
  onFocusTitle,
  onFocusDescription,
}: TicketPropertiesSidebarProps) {
  const { t } = useTranslation();
  const { addToast } = useToastStore();
  const { projectList } = useProject();
  const mutations = useTicketPropertyMutations(ticketId);

  const [openField, setOpenField] = useState<TicketPickerField | null>(null);
  const [anchorRect, setAnchorRect] = useState<DOMRect | null>(null);

  const openPicker = useCallback((field: TicketPickerField, rect: DOMRect | null) => {
    setOpenField(field);
    setAnchorRect(rect);
  }, []);

  const closePicker = useCallback(() => {
    setOpenField(null);
    setAnchorRect(null);
  }, []);

  const onHotkey = useCallback(
    (action: TicketHotkeyAction) => {
      if (action === 'focusTitle') {
        onFocusTitle();
        return;
      }
      if (action === 'focusDescription') {
        onFocusDescription();
        return;
      }
      openPicker(action, null);
    },
    [onFocusTitle, onFocusDescription, openPicker],
  );

  useTicketDetailHotkeys({ enabled: true, onAction: onHotkey });

  const { data: workflowStatuses = [] } = useWorkflowStatuses(
    ticket.project ?? undefined,
    ticket.project ? undefined : ticket.team?.id,
  );

  const usersEnabled =
    ticketUserOptionsEnabled({
      projectId: ticket.project,
      teamId: ticket.team?.id,
    }) &&
    (openField === 'assignee' || openField === 'reviewer');

  const { data: userOptions = [], isLoading: usersLoading } = useQuery({
    queryKey: ['users', ticket.project ?? null, ticket.team?.id ?? null],
    queryFn: () =>
      fetchTicketUserOptions(apiClient, {
        projectId: ticket.project,
        teamId: ticket.team?.id,
      }),
    enabled: usersEnabled,
  });

  const { data: labelOptions = [], isLoading: labelsLoading } = useQuery({
    queryKey: ['labels', ticket.project ?? null, ticket.team?.id ?? null],
    queryFn: async () => {
      const res = await apiClient.get<{ results?: { id: number; name: string; color: string }[] }>(
        '/labels/',
        {
          params: {
            ...(ticket.project ? { project: ticket.project } : {}),
            ...(!ticket.project && ticket.team?.id ? { team: ticket.team.id } : {}),
          },
        },
      );
      return res.data.results ?? [];
    },
    enabled: openField === 'labels' && (!!ticket.project || !!ticket.team?.id),
  });

  const { data: cycles = [], isLoading: cyclesLoading } = useQuery({
    queryKey: ['cycles', ticket.project ?? null, ticket.team?.id ?? null],
    queryFn: async () => {
      const params: Record<string, number> = {};
      if (ticket.project) params.project = ticket.project;
      if (ticket.team?.id) params.team = ticket.team.id;
      const res = await apiClient.get('/cycles/', { params });
      return (res.data.results ?? res.data) as { id: number; name: string }[];
    },
    enabled: openField === 'cycle' && (!!ticket.project || !!ticket.team?.id),
  });

  const { data: categories = [], isLoading: categoriesLoading } = useQuery({
    queryKey: ['categories'],
    queryFn: async () => {
      const res = await apiClient.get<{ id: number; name: string; color: string }[]>('/categories/');
      return res.data;
    },
    enabled: openField === 'category',
  });

  const { data: milestones = [], isLoading: milestonesLoading } = useQuery({
    queryKey: ['milestones'],
    queryFn: async () => {
      const res = await apiClient.get<
        { results?: { id: number; name: string; dueDate: string | null }[] } & {
          id: number;
          name: string;
          dueDate: string | null;
        }[]
      >('/milestones/');
      return res.data.results ?? (Array.isArray(res.data) ? res.data : []);
    },
    enabled: openField === 'milestone',
  });

  const projectLabel =
    ticket.projectName
    || ticket.projectPrefix
    || (ticket.project != null ? String(ticket.project) : null);
  const wikiLabel =
    ticket.linkedWikiPages && ticket.linkedWikiPages.length > 0
      ? ticket.linkedWikiPages.map((w) => w.title).join(', ')
      : null;

  const pickerOptions: TicketFieldPickerOption[] = useMemo(() => {
    switch (openField) {
      case 'status':
        return workflowStatuses.map((s) => ({ id: s.slug, label: s.name }));
      case 'assignee':
      case 'reviewer':
        return userOptions.map((u) => ({
          id: String(u.id),
          label: u.displayName || u.username,
          hint: u.username,
        }));
      case 'priority':
        return PRIORITIES.map((p) => ({ id: p, label: p }));
      case 'cycle':
        return cycles.map((c) => ({ id: String(c.id), label: c.name }));
      case 'project':
        return projectList.map((p) => ({
          id: String(p.id),
          label: p.name,
          hint: p.prefix,
        }));
      case 'labels':
        return labelOptions.map((l) => ({ id: String(l.id), label: l.name }));
      case 'estimate':
        return ESTIMATES.map((n) => ({ id: String(n), label: String(n) }));
      case 'category':
        return categories.map((c) => ({ id: String(c.id), label: c.name }));
      case 'milestone':
        return milestones.map((m) => ({ id: String(m.id), label: m.name }));
      default:
        return [];
    }
  }, [
    openField,
    workflowStatuses,
    userOptions,
    cycles,
    projectList,
    labelOptions,
    categories,
    milestones,
  ]);

  const selectedIds = useMemo(() => {
    switch (openField) {
      case 'status':
        return ticket.status ? [ticket.status] : [];
      case 'assignee':
        return ticket.assignees.map((a) => String(a.id));
      case 'reviewer':
        return ticket.reviewers.map((r) => String(r.id));
      case 'priority':
        return ticket.priority ? [ticket.priority] : [];
      case 'cycle':
        return ticket.cycle != null ? [String(ticket.cycle)] : [];
      case 'project':
        return ticket.project != null ? [String(ticket.project)] : [];
      case 'labels':
        return ticket.labels.map((l) => String(l.id));
      case 'estimate':
        return ticket.storyPoints != null ? [String(ticket.storyPoints)] : [];
      case 'category':
        return ticket.category ? [String(ticket.category.id)] : [];
      case 'milestone':
        return ticket.milestone ? [String(ticket.milestone.id)] : [];
      default:
        return [];
    }
  }, [openField, ticket]);

  const pickerLoading =
    (openField === 'assignee' || openField === 'reviewer') && usersLoading
      ? true
      : openField === 'labels' && labelsLoading
        ? true
        : openField === 'cycle' && cyclesLoading
          ? true
          : openField === 'category' && categoriesLoading
            ? true
            : openField === 'milestone' && milestonesLoading;

  const handleConfirm = (ids: string[]) => {
    if (!openField) return;
    switch (openField) {
      case 'status': {
        const slug = ids[0];
        if (slug) mutations.status.mutate(slug);
        break;
      }
      case 'priority': {
        const p = ids[0];
        if (p) mutations.priority.mutate(p);
        break;
      }
      case 'estimate': {
        if (!ids.length || ids[0] === '0') mutations.storyPoints.mutate(null);
        else mutations.storyPoints.mutate(Number(ids[0]));
        break;
      }
      case 'cycle': {
        if (!ids.length || ids[0] === '0') {
          mutations.cycle.mutate(null);
        } else {
          const c = cycles.find((x) => String(x.id) === ids[0]);
          mutations.cycle.mutate(c ? { id: c.id, name: c.name } : Number(ids[0]));
        }
        break;
      }
      case 'project': {
        addToast({ message: t('ticketDetail.projectInlineUnavailable'), type: 'info' });
        break;
      }
      case 'assignee': {
        if (ids.includes('0') || ids.length === 0) {
          mutations.assignees.mutate([]);
        } else {
          const next = userOptions
            .filter((u) => ids.includes(String(u.id)))
            .map((u) => ({
              id: u.id,
              username: u.username,
              displayName: u.displayName,
            }));
          mutations.assignees.mutate(next);
        }
        break;
      }
      case 'reviewer': {
        if (ids.includes('0') || ids.length === 0) {
          mutations.reviewers.mutate([]);
        } else {
          const next = userOptions
            .filter((u) => ids.includes(String(u.id)))
            .map((u) => ({
              id: u.id,
              username: u.username,
              displayName: u.displayName,
            }));
          mutations.reviewers.mutate(next);
        }
        break;
      }
      case 'labels': {
        const next = labelOptions.filter((l) => ids.includes(String(l.id)));
        mutations.labels.mutate(next);
        break;
      }
      case 'category': {
        if (!ids.length || ids[0] === '0') mutations.category.mutate(null);
        else {
          const c = categories.find((x) => String(x.id) === ids[0]);
          mutations.category.mutate(c ?? null);
        }
        break;
      }
      case 'milestone': {
        if (!ids.length || ids[0] === '0') mutations.milestone.mutate(null);
        else {
          const m = milestones.find((x) => String(x.id) === ids[0]);
          mutations.milestone.mutate(
            m ? { id: m.id, name: m.name, dueDate: m.dueDate ?? null } : null,
          );
        }
        break;
      }
      default:
        break;
    }
  };

  const row = (
    field: TicketPickerField | null,
    label: string,
    value: string,
    editable: boolean,
  ) => {
    if (!editable || !field) {
      return (
        <div className="ticket-property-item">
          <span className="ticket-property-label">{label}</span>
          <span className="ticket-property-value">{value}</span>
        </div>
      );
    }
    return (
      <button
        type="button"
        className="ticket-property-item ticket-property-item--button"
        onClick={(e) => openPicker(field, e.currentTarget.getBoundingClientRect())}
        data-testid={`ticket-property-${field}`}
      >
        <span className="ticket-property-label">{label}</span>
        <span className="ticket-property-value">{value}</span>
      </button>
    );
  };

  const isDateField = openField === 'dueDate' || openField === 'startDate';

  return (
    <>
      <div className="ticket-properties-sidebar">
        <h3 className="ticket-properties-sidebar__title">Properties</h3>
        <div className="ticket-properties-list">
          {row('status', 'Status', ticket.status || '—', true)}
          {row(
            'assignee',
            'Assignee',
            ticket.assignees.length > 0
              ? ticket.assignees.map((a) => a.displayName || a.username).join(', ')
              : '—',
            true,
          )}
          {row('priority', 'Priority', ticket.priority || '—', true)}
          {row('cycle', 'Cycle', ticket.cycleName || '—', true)}
          {row('project', 'Project', projectLabel || '—', true)}
          {row(
            'labels',
            'Labels',
            ticket.labels.length > 0 ? ticket.labels.map((l) => l.name).join(', ') : '—',
            true,
          )}
          {row(
            'estimate',
            'Estimate',
            ticket.storyPoints != null ? String(ticket.storyPoints) : '—',
            true,
          )}
          {row('dueDate', 'Due Date', formatDate(ticket.dueDate), true)}
        </div>
      </div>

      <div className="ticket-properties-sidebar">
        <h3 className="ticket-properties-sidebar__title">Others</h3>
        <div className="ticket-properties-list">
          {row(null, 'Author', ticket.author?.displayName || ticket.author?.username || '—', false)}
          {row('startDate', 'Start date', formatDate(ticket.startDate), true)}
          {row('category', 'Category', ticket.category?.name || '—', true)}
          {row('milestone', 'Milestone', ticket.milestone?.name || '—', true)}
          {row(null, 'Linked Wiki', wikiLabel || '—', false)}
          {row(
            'reviewer',
            'Reviewer',
            ticket.reviewers.length > 0
              ? ticket.reviewers.map((r) => r.displayName || r.username).join(', ')
              : '—',
            true,
          )}
        </div>
      </div>

      {openField && (
        <TicketFieldPicker
          field={openField}
          open
          onClose={closePicker}
          placement={openField === 'status' ? 'center' : 'anchor'}
          anchorRect={anchorRect}
          options={pickerOptions}
          multi={openField === 'assignee' || openField === 'labels' || openField === 'reviewer'}
          selectedIds={selectedIds}
          onConfirm={handleConfirm}
          loading={pickerLoading}
          allowEmpty={
            openField === 'assignee' ||
            openField === 'reviewer' ||
            openField === 'estimate' ||
            openField === 'cycle' ||
            openField === 'category' ||
            openField === 'milestone'
          }
          emptyLabel="Unassigned"
          title={PICKER_FIELD_TITLES[openField]}
          dateMode={isDateField}
          dateValue={openField === 'dueDate' ? ticket.dueDate : openField === 'startDate' ? ticket.startDate : null}
          onConfirmDate={(value) => {
            if (openField === 'dueDate') mutations.dueDate.mutate(value);
            if (openField === 'startDate') mutations.startDate.mutate(value);
          }}
        />
      )}
    </>
  );
}
