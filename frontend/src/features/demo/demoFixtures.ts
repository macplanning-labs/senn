/**
 * demoFixtures.ts — デモ用 fixture データ
 *
 * ログイン後に表示する最小限のサンプルデータ。
 */

/** authStore User と同じ camelCase 形状 */
export const demoUser = {
  id: 1,
  username: 'demo',
  email: 'demo@senn-app.local',
  firstName: 'Demo',
  lastName: 'User',
  displayName: 'Demo User',
  alias: null as string | null,
  isStaff: false,
  emailNotificationsEnabled: true,
};

const demoAuthor = {
  id: 1,
  username: 'demo',
  displayName: 'Demo User',
};

/** MyIssuesPage が期待する snake_case 配列 */
export const demoMyTickets = [
  {
    id: 101,
    ticket_key: 'DEMO-1',
    title: 'デモ: ダッシュボードを改善する',
    status: 'in_progress',
    priority: 'high',
    due_date: '2026-09-30',
    updated_at: '2026-09-17T08:00:00Z',
    project_key: 'DEMO',
  },
  {
    id: 102,
    ticket_key: 'DEMO-2',
    title: 'デモ: ログイン画面にデモボタンを追加',
    status: 'open',
    priority: 'medium',
    due_date: null,
    updated_at: '2026-09-15T10:30:00Z',
    project_key: 'DEMO',
  },
  {
    id: 103,
    ticket_key: 'DEMO-3',
    title: 'デモ: API クライアントのテストを修正',
    status: 'open',
    priority: 'medium',
    due_date: '2026-09-25',
    updated_at: '2026-09-14T15:00:00Z',
    project_key: 'DEMO',
  },
  {
    id: 104,
    ticket_key: 'DEMO-4',
    title: 'デモ: ドキュメントのスモーク手順を更新',
    status: 'in_progress',
    priority: 'low',
    due_date: null,
    updated_at: '2026-09-10T12:00:00Z',
    project_key: 'DEMO',
  },
  {
    id: 105,
    ticket_key: 'DEMO-5',
    title: 'デモ: MainLayout にバナーを表示',
    status: 'in_progress',
    priority: 'high',
    due_date: '2026-09-20',
    updated_at: '2026-09-16T14:00:00Z',
    project_key: 'DEMO',
  },
];

export const demoProject = {
  id: 1,
  name: 'Sample Project',
  key: 'DEMO',
  prefix: 'DEMO',
  projectKey: 'DEMO',
  description: 'Sample project with demo tickets',
};

export const demoTeam = {
  id: 1,
  name: 'Demo Team',
  slug: 'demo-team',
  description: 'Sample team for demo experience',
  icon: '👨‍💻',
  color: '#007AFF',
  isActive: true,
  memberCount: 1,
  projectCount: 1,
  createdAt: '2026-01-01T00:00:00Z',
  slackWebhookUrl: '',
};

const ticketDescriptions: Record<string, string> = {
  'DEMO-1': 'ダッシュボードのウィジェット配置と表示速度を改善するデモ用チケットです。',
  'DEMO-2': 'ログイン画面から登録なしで体験できるデモモード入口を追加するタスクです。',
  'DEMO-3': 'API クライアントのインターセプターと fixture 返却のテストを整備します。',
  'DEMO-4': 'デモ体験向けのスモーク手順ドキュメントを最新化します。',
  'DEMO-5': 'デモモード中であることを示すバナーを MainLayout に表示します。',
};

const ticketTypes: Record<string, string> = {
  'DEMO-1': 'task',
  'DEMO-2': 'feature',
  'DEMO-3': 'bug',
  'DEMO-4': 'task',
  'DEMO-5': 'feature',
};

/** TicketDetailPanel TicketData 形状（camelCase） */
export function getDemoTicketDetail(keyOrId: string | number) {
  const lookup = String(keyOrId);
  const ticket = demoMyTickets.find((t) => {
    if (t.ticket_key.toLowerCase() === lookup.toLowerCase()) {
      return true;
    }
    if (/^\d+$/.test(lookup) && t.id === Number(lookup)) {
      return true;
    }
    return false;
  });

  if (!ticket) {
    return null;
  }

  return {
    id: ticket.id,
    ticketKey: ticket.ticket_key,
    title: ticket.title,
    description: ticketDescriptions[ticket.ticket_key] ?? 'デモ用サンプルチケットです。',
    status: ticket.status,
    priority: ticket.priority,
    ticketType: ticketTypes[ticket.ticket_key] ?? 'task',
    assignees: [demoAuthor],
    reviewers: [],
    author: demoAuthor,
    category: null,
    milestone: null,
    project: demoProject.id,
    projectPrefix: 'DEMO',
    team: { id: demoTeam.id, slug: demoTeam.slug, name: demoTeam.name },
    parent: null,
    startDate: null,
    dueDate: ticket.due_date,
    closedAt: null,
    createdAt: '2026-09-10T00:00:00Z',
    updatedAt: ticket.updated_at,
    storyPoints: null,
    commentCount: 0,
    childCount: 0,
    comments: [],
    attachments: [],
    links: [],
    labels: [],
    isWatching: false,
  };
}
