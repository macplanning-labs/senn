/**
 * demoData.ts — サンプルデータ生成関数と定義
 *
 * SENN にサンプルプロジェクト・チケット・依存関係を生成する。
 * バックエンド変更なし。失敗時は巻き戻す。
 */

import i18n from '@/i18n';
import { apiClient } from '@/shared/api/client';

/** チケット定義 */
interface TicketDef {
  titleJa: string;
  titleEn: string;
  status: string;
  priority: string;
  startDayOffset: number;
  dueDayOffset: number;
}

/** サンプルデータ定義（8 チケット） */
const DEMO_TICKETS: TicketDef[] = [
  {
    titleJa: '要件を整理する',
    titleEn: 'Define requirements',
    // 新規チームの既定ステータス（backlog/open/in_progress/resolved/closed/canceled）に 'done' は無い
    status: 'resolved',
    priority: 'high',
    startDayOffset: 0,
    dueDayOffset: 2,
  },
  {
    titleJa: '画面デザインを作る',
    titleEn: 'Design screens',
    status: 'in_progress',
    priority: 'high',
    startDayOffset: 2,
    dueDayOffset: 6,
  },
  {
    titleJa: 'データ設計をする',
    titleEn: 'Design data model',
    status: 'in_progress',
    priority: 'medium',
    startDayOffset: 2,
    dueDayOffset: 5,
  },
  {
    titleJa: '画面を実装する',
    titleEn: 'Build screens',
    status: 'open',
    priority: 'high',
    startDayOffset: 6,
    dueDayOffset: 12,
  },
  {
    titleJa: 'API を実装する',
    titleEn: 'Build API',
    status: 'open',
    priority: 'medium',
    startDayOffset: 5,
    dueDayOffset: 11,
  },
  {
    titleJa: '結合テストをする',
    titleEn: 'Integration test',
    status: 'open',
    priority: 'medium',
    startDayOffset: 12,
    dueDayOffset: 15,
  },
  {
    titleJa: 'ドキュメントを書く',
    titleEn: 'Write docs',
    status: 'open',
    priority: 'low',
    startDayOffset: 8,
    dueDayOffset: 14,
  },
  {
    titleJa: 'リリースする',
    titleEn: 'Release',
    status: 'open',
    priority: 'urgent',
    startDayOffset: 15,
    dueDayOffset: 16,
  },
];

/** 依存関係定義（from → to、0-based インデックス） */
const DEMO_DEPENDENCIES: [number, number][] = [
  [0, 1], // 0 → 1
  [0, 2], // 0 → 2
  [1, 3], // 1 → 3
  [2, 4], // 2 → 4
  [3, 5], // 3 → 5
  [4, 5], // 4 → 5
  [5, 7], // 5 → 7
];

/** 日付を YYYY-MM-DD にローカル時間で整形 */
export function formatDateLocal(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

/** 今日の日付にオフセット日数を加算 */
function getDateWithOffset(dayOffset: number): string {
  const today = new Date();
  today.setDate(today.getDate() + dayOffset);
  return formatDateLocal(today);
}

/** 未使用 prefix を決定（DEMO、DEMO2〜DEMO20） */
function findAvailablePrefix(existingPrefixes: string[]): string {
  const upperPrefixes = existingPrefixes.map((p) => p.toUpperCase());

  // DEMO が未使用なら使う
  if (!upperPrefixes.includes('DEMO')) {
    return 'DEMO';
  }

  // DEMO2 から DEMO20 を試す
  for (let i = 2; i <= 20; i++) {
    const candidate = `DEMO${i}`;
    if (!upperPrefixes.includes(candidate)) {
      return candidate;
    }
  }

  throw new Error('All DEMO prefixes (DEMO to DEMO20) are in use');
}

/** プロジェクトがデモプロジェクトかを判定 */
export function isDemoProject(project: {
  prefix: string;
  name: string;
}): boolean {
  const prefixMatch = /^DEMO\d*$/i.test(project.prefix);
  const nameMatch =
    project.name === 'サンプルプロジェクト' || project.name === 'Sample project';
  return prefixMatch && nameMatch;
}

/** チケット一括削除してからプロジェクトを削除 */
export async function deleteDemoProject(projectId: number): Promise<void> {
  // Step 1: チケット一括削除
  // 失敗したら project delete を呼ばず、例外を投げる
  await apiClient.post('/tickets/bulk-delete/', {
    project_id: projectId,
    delete_all: true,
  });

  // Step 2: プロジェクト削除
  await apiClient.delete(`/projects/${projectId}/`);
}

/** サンプルデータ生成 */
export async function createDemoData(
  teamId: number,
  existingPrefixes: string[],
): Promise<{ prefix: string; projectId: number }> {
  const prefix = findAvailablePrefix(existingPrefixes);
  const isJa = i18n.language.startsWith('ja');
  const description = isJa
    ? 'SENN のサンプルチケットです。不要になったらプロジェクトごと削除できます。'
    : 'A sample ticket from SENN. Delete the whole project when you no longer need it.';

  const projectName = isJa ? 'サンプルプロジェクト' : 'Sample project';

  let projectId: number | undefined;

  try {
    // Step 1: プロジェクト作成
    const projectRes = await apiClient.post<{ id: number }>('/projects/', {
      name: projectName,
      prefix,
      description,
      priority: 'medium',
      teamIds: [teamId],
    });
    projectId = projectRes.data.id;

    // Step 2: Cycle 作成
    const cycleRes = await apiClient.post<{ id: number }>('/cycles/', {
      project: projectId,
      name: 'Sprint 1',
      start_date: getDateWithOffset(0),
      end_date: getDateWithOffset(16),
      // teamId を明示する（project だけだとバックエンドが参加チームから補完する経路を通る）
      teamId,
    });
    const cycleId = cycleRes.data.id;

    // Step 3: チケット作成（逐次実行）
    const createdTickets: Array<{ id: number; ticketKey: string }> = [];

    for (const ticketDef of DEMO_TICKETS) {
      const title = isJa ? ticketDef.titleJa : ticketDef.titleEn;
      const ticketRes = await apiClient.post<{ id: number; ticketKey: string }>(
        '/tickets/',
        {
          title,
          description,
          project: projectId,
          status: ticketDef.status,
          priority: ticketDef.priority,
          ticket_type: 'task',
          start_date: getDateWithOffset(ticketDef.startDayOffset),
          due_date: getDateWithOffset(ticketDef.dueDayOffset),
          cycle: cycleId,
          assignees: [],
          labels: [],
          teamId,
        },
      );
      createdTickets.push({
        id: ticketRes.data.id,
        ticketKey: ticketRes.data.ticketKey,
      });
    }

    // Step 4: 依存関係作成
    for (const [fromIdx, toIdx] of DEMO_DEPENDENCIES) {
      const fromTicket = createdTickets[fromIdx];
      const toTicket = createdTickets[toIdx];
      if (!fromTicket || !toTicket) {
        throw new Error('demo dependency index out of range');
      }
      await apiClient.post(
        `/tickets/${fromTicket.ticketKey}/dependencies/`,
        {
          to_task: toTicket.id,
          dependency_type: 'blocks',
        },
      );
    }

    return { prefix, projectId };
  } catch (error) {
    // いずれかの step で失敗したら巻き戻す
    if (projectId) {
      try {
        await deleteDemoProject(projectId);
      } catch {
        // 巻き戻し失敗でも元の例外を投げる
      }
    }
    throw error;
  }
}
