/**
 * activityMessage.ts — Activity イベントを、表示用の i18n キー＋パラメータに変換する純粋関数。
 *
 * 1つのイベントが複数行になる場合がある(project_updated は変更項目ごとに1行)。
 * 文言そのものは i18n.ts の `projectActivity.*` に置く(ここでは組み立てない)。
 */

import type { ProjectActivityEvent } from '../hooks/useProjectActivity';

export interface ActivityLine {
  /** i18n キー(`projectActivity.` 配下) */
  key: string;
  params?: Record<string, string | number>;
}

type Change = { field?: string; from?: unknown; to?: unknown };

const str = (v: unknown): string => (v === null || v === undefined || v === '' ? '' : String(v));

/** 画面の見た目に使う種別(アイコン・色分け用) */
export function activityCategory(eventType: string): 'project' | 'team' | 'milestone' | 'ticket' | 'update' {
  if (eventType.startsWith('team_')) return 'team';
  if (eventType.startsWith('milestone_')) return 'milestone';
  if (eventType.startsWith('ticket_')) return 'ticket';
  if (eventType === 'update_posted') return 'update';
  // Phase 3 の新しいイベント（親子・関連・ロードマップ）も project カテゴリ
  if (eventType.startsWith('parent_') || eventType.startsWith('child_') ||
      eventType.startsWith('relation_') || eventType.startsWith('roadmap_')) return 'project';
  return 'project';
}

function changeLine(c: Change): ActivityLine {
  const from = str(c.from);
  const to = str(c.to);
  switch (c.field) {
    case 'name':
      return { key: 'changeName', params: { from, to } };
    case 'description':
      return { key: 'changeDescription' };
    case 'status':
      return { key: 'changeStatus', params: { from, to } };
    case 'priority':
      return { key: 'changePriority', params: { from, to } };
    case 'targetEndDate':
      // 期日の設定/解除は、空を「未設定」として表示するため専用キーに分ける
      if (!from) return { key: 'changeTargetDateSet', params: { to } };
      if (!to) return { key: 'changeTargetDateCleared', params: { from } };
      return { key: 'changeTargetDate', params: { from, to } };
    case 'ownerId':
      return { key: 'changeOwner' };
    default:
      return { key: 'changeOther', params: { field: str(c.field) } };
  }
}

export function describeActivity(event: ProjectActivityEvent): ActivityLine[] {
  const p = event.payload ?? {};
  switch (event.eventType) {
    case 'project_created':
      return [{ key: 'projectCreated', params: { name: str(p.name) } }];
    case 'project_updated': {
      const changes = Array.isArray(p.changes) ? (p.changes as Change[]) : [];
      return changes.length > 0 ? changes.map(changeLine) : [{ key: 'changeOther', params: { field: '' } }];
    }
    case 'team_added':
      return [{ key: 'teamAdded', params: { name: str(p.team_name) } }];
    case 'team_removed':
      return [{ key: 'teamRemoved', params: { name: str(p.team_name) } }];
    case 'milestone_created':
      return [{ key: 'milestoneCreated', params: { name: str(p.name) } }];
    case 'milestone_updated': {
      const changes = Array.isArray(p.changes) ? (p.changes as Change[]) : [];
      const lines = changes.map((c): ActivityLine =>
        c.field === 'dueDate'
          ? { key: 'milestoneDueChanged', params: { name: str(p.name), from: str(c.from), to: str(c.to) } }
          : { key: 'milestoneRenamed', params: { from: str(c.from), to: str(c.to) } },
      );
      return lines.length > 0 ? lines : [{ key: 'milestoneUpdated', params: { name: str(p.name) } }];
    }
    case 'milestone_deleted':
      return [{ key: 'milestoneDeleted', params: { name: str(p.name) } }];
    case 'ticket_added':
      return [{ key: 'ticketAdded', params: { ticket: str(p.ticket_key), title: str(p.title) } }];
    case 'ticket_removed':
      return [{ key: 'ticketRemoved', params: { ticket: str(p.ticket_key), title: str(p.title) } }];
    case 'ticket_completed':
      return [{ key: 'ticketCompleted', params: { ticket: str(p.ticket_key), title: str(p.title) } }];
    case 'update_posted':
      return [{ key: 'updatePosted', params: { health: str(p.health) } }];
    // Phase 3: 親子・関連・ロードマップ
    case 'parent_changed': {
      const from = p.from as { id?: number; prefix?: string; name?: string } | null;
      const to = p.to as { id?: number; prefix?: string; name?: string } | null;
      const fromName = from ? `${from.prefix || ''} ${from.name || ''}`.trim() : null;
      const toName = to ? `${to.prefix || ''} ${to.name || ''}`.trim() : null;
      return [{ key: 'parentChanged', params: { from: fromName || '', to: toName || '' } }];
    }
    case 'child_added': {
      const proj = p.project as { prefix?: string; name?: string } | undefined;
      const projName = proj ? `${proj.prefix || ''} ${proj.name || ''}`.trim() : '';
      return [{ key: 'childAdded', params: { project: projName } }];
    }
    case 'child_removed': {
      const proj = p.project as { prefix?: string; name?: string } | undefined;
      const projName = proj ? `${proj.prefix || ''} ${proj.name || ''}`.trim() : '';
      return [{ key: 'childRemoved', params: { project: projName } }];
    }
    case 'relation_added': {
      const proj = p.project as { prefix?: string; name?: string } | undefined;
      const projName = proj ? `${proj.prefix || ''} ${proj.name || ''}`.trim() : '';
      return [{ key: 'relationAdded', params: { project: projName } }];
    }
    case 'relation_removed': {
      const proj = p.project as { prefix?: string; name?: string } | undefined;
      const projName = proj ? `${proj.prefix || ''} ${proj.name || ''}`.trim() : '';
      return [{ key: 'relationRemoved', params: { project: projName } }];
    }
    case 'roadmap_added': {
      const roadmap = p.roadmap as { name?: string } | undefined;
      return [{ key: 'roadmapAdded', params: { name: str(roadmap?.name) } }];
    }
    case 'roadmap_removed': {
      const roadmap = p.roadmap as { name?: string } | undefined;
      return [{ key: 'roadmapRemoved', params: { name: str(roadmap?.name) } }];
    }
    default:
      // 将来追加されるイベント種別でも画面が壊れないようにする
      return [{ key: 'unknown', params: { type: event.eventType } }];
  }
}

/** 日付ごとのグループ見出し用キー(ローカル日付 YYYY-MM-DD) */
export function dayKey(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
