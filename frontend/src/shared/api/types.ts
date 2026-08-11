/**
 * API 型定義 — 手書きの型定義（orval 生成前の暫定版）
 *
 * Phase E で orval による自動生成に移行するまでの間、
 * 各コンポーネントで散在している型定義をここに集約する。
 *
 * 移行後は src/generated/api/model/ のファイルが正典となり、
 * このファイルは re-export のみを行うファサードに変わる。
 */

// ============================================================
// チケット
// ============================================================

export interface Ticket {
  id: number;
  ticketKey: string;
  title: string;
  description: string;
  status: TicketStatus;
  priority: Priority;
  ticketType: TicketType;
  assignees: UserSummary[];
  reporter: UserSummary | null;
  project: number;
  parent: number | null;
  category: number | null;
  milestone: number | null;
  labels: Label[];
  dueDate: string | null;
  startDate: string | null;
  storyPoints: number | null;
  estimatedHours: number | null;
  actualHours: number | null;
  commentCount: number;
  comments: Comment[];
  createdAt: string;
  updatedAt: string;
  closedAt: string | null;
  assignedTeam: TeamSummary | null;
  cycle: number | null;
  cycleName: string | null;
}

export type TicketStatus = 'backlog' | 'open' | 'in_progress' | 'resolved' | 'closed' | 'canceled';
export type Priority = 'urgent' | 'high' | 'medium' | 'low';
export type TicketType = 'task' | 'bug' | 'feature' | 'improvement';

export interface TicketListItem {
  id: number;
  ticketKey: string;
  title: string;
  status: TicketStatus;
  priority: Priority;
  ticketType: TicketType;
  assignees: UserSummary[];
  dueDate: string | null;
  storyPoints: number | null;
  labels: Label[];
  commentCount: number;
  updatedAt: string;
  cycle: number | null;
  cycleName: string | null;
  totalTimeSpent: number;
  assignedTeam: TeamSummary | null;
}

// ============================================================
// ユーザー
// ============================================================

export interface UserSummary {
  id: number;
  username: string;
  firstName: string;
  lastName: string;
  displayName: string;
}

// ============================================================
// コメント
// ============================================================

export interface Comment {
  id: number;
  body: string;
  author: UserSummary;
  createdAt: string;
}

// ============================================================
// プロジェクト
// ============================================================

export type ProjectStatus = 'active' | 'completed' | 'paused' | 'canceled';

export interface Project {
  id: number;
  name: string;
  projectKey: string;
  description: string;
  status: ProjectStatus;
  targetStartDate: string | null;
  targetEndDate: string | null;
  isActive: boolean;
  gracePeriodDays: number;
  ownerTeam: TeamSummary | null;
  createdAt: string;
}

// ============================================================
// ラベル
// ============================================================

export interface Label {
  id: number;
  name: string;
  color: string;
  project: number;
}

// ============================================================
// 通知
// ============================================================

export type NotificationCategory =
  | 'assigned'
  | 'commented'
  | 'status_changed'
  | 'due_soon'
  | 'overdue'
  | 'mentioned'
  | 'wiki_updated';

export interface Notification {
  id: number;
  category: NotificationCategory;
  title: string;
  message: string;
  ticketKey: string | null;
  wikiTitle: string | null;
  isRead: boolean;
  createdAt: string;
}

// ============================================================
// ダッシュボード
// ============================================================

export interface DashboardStats {
  openTickets: number;
  overdueTickets: number;
  completedThisWeek: number;
  totalProjects: number;
  dueSoonTickets: number;
}

export interface ActivityItem {
  id: number;
  ticketKey: string | null;
  ticketTitle: string | null;
  oldStatus: TicketStatus;
  newStatus: TicketStatus;
  changedBy: string;
  changedAt: string;
}

// ============================================================
// マイルストーン
// ============================================================

export interface Milestone {
  id: number;
  name: string;
  description: string;
  dueDate: string | null;
  project: number;
  progress: number;
}

// ============================================================
// カテゴリ
// ============================================================

export interface Category {
  id: number;
  name: string;
  slug: string;
  level: 1 | 2;
  parent: number | null;
  project: number;
}

// ============================================================
// メンバーシップ
// ============================================================

export interface Membership {
  id: number;
  user: UserSummary;
  project: number;
  startDate: string;
  endDate: string | null;
  note: string;
  addedBy: UserSummary | null;
}

// ============================================================
// タスク依存関係
// ============================================================

export type DependencyType = 'blocks' | 'relates_to';

export interface TaskDependency {
  id: number;
  fromTask: number;
  fromTaskKey: string;
  fromTaskTitle: string;
  toTask: number;
  toTaskKey: string;
  toTaskTitle: string;
  dependencyType: DependencyType;
  createdBy: UserSummary;
  createdAt: string;
}

// ============================================================
// サイクル（スプリント）
// ============================================================

export type CycleStatus = 'planned' | 'active' | 'completed';

export interface Cycle {
  id: number;
  project: number;
  name: string;
  number: number;
  status: CycleStatus;
  startDate: string;
  endDate: string;
  createdBy: UserSummary | null;
  createdAt: string;
  ticketCount: number;
  completedCount: number;
  totalPoints: number;
  completedPoints: number;
}

export interface CycleProgress {
  ticketCount: number;
  completedCount: number;
  inProgressCount: number;
  totalPoints: number;
  completedPoints: number;
  completionRate: number;
  initialPoints: number;
  scopeAdded: number;
  scopeRemoved: number;
  scopeChange: number;
}

export interface VelocityData {
  cycleId: number;
  cycleNumber: number;
  cycleName: string;
  completedCount: number;
  completedPoints: number;
  scopeChange: number;
  carryOver: number;
}

export interface BurndownPoint {
  date: string;
  ideal: number;
  actual: number;
  totalScope: number;
}

// ============================================================
// タイムエントリ（作業時間記録）
// ============================================================

export interface TimeEntry {
  id: number;
  ticket: number;
  user: UserSummary;
  description: string;
  startTime: string | null;
  endTime: string | null;
  durationMinutes: number;
  createdAt: string;
}

// ============================================================
// ワークフローステータス（プロジェクト別カスタムステータス）
// ============================================================

export type StatusCategory = 'backlog' | 'unstarted' | 'started' | 'completed' | 'cancelled';

export interface WorkflowStatus {
  id: number;
  project: number;
  name: string;
  slug: string;
  category: StatusCategory;
  color: string;
  position: number;
  isDefault: boolean;
}

// ============================================================
// Git連携
// ============================================================

export type GitProvider = 'github' | 'gitlab';
export type GitEventType = 'commit' | 'pull_request' | 'branch';
export type PRState = 'open' | 'merged' | 'closed';

export interface GitIntegration {
  id: number;
  project: number;
  provider: GitProvider;
  repositoryUrl: string;
  webhookSecret: string;
  isActive: boolean;
  createdBy: UserSummary | null;
  createdAt: string;
  eventCount: number;
}

export interface GitEvent {
  id: number;
  eventType: GitEventType;
  title: string;
  url: string;
  sha: string;
  shaShort: string;
  branch: string;
  authorName: string;
  authorAvatarUrl: string;
  prNumber: number | null;
  prState: PRState;
  createdAt: string;
}

// ============================================================
// メール通知設定
// ============================================================

export interface EmailPreferences {
  emailNotificationsEnabled: boolean;
  notifyOnAssigned: boolean;
  notifyOnCommented: boolean;
  notifyOnStatusChanged: boolean;
  notifyOnDueSoon: boolean;
}

// ============================================================
// チーム
// ============================================================

export interface TeamSummary {
  id: number;
  name: string;
  slug: string;
  icon: string;
  color: string;
}

export interface Team {
  id: number;
  name: string;
  slug: string;
  description: string;
  icon: string;
  color: string;
  slackWebhookUrl: string;
  isActive: boolean;
  memberCount: number;
  projectCount: number;
  createdAt: string;
}

export type TeamRole = 'leader' | 'member';

export interface TeamMembership {
  id: number;
  team: number;
  user: UserSummary;
  role: TeamRole;
  joinedAt: string;
}

// ============================================================
// Phase 3: 運用系
// ============================================================

export interface TaskPointHistory {
  id: number;
  old_points: number | null;
  new_points: number | null;
  changedBy: UserSummary | null;
  reason: string;
  changedAt: string;
}

export type TriageStatus = 'pending' | 'approved' | 'rejected';
export type ChangeType = 'text_request' | 'master_change';

export interface TriageRequest {
  id: number;
  title: string;
  description: string;
  changeType: ChangeType;
  changePayload: Record<string, unknown>;
  status: TriageStatus;
  project: number | null;
  ticket: number | null;
  ticketKey: string | null;
  ticketId: number | null;
  requestedBy: UserSummary;
  reviewedBy: UserSummary | null;
  reviewedAt: string | null;
  reviewComment: string;
  createdAt: string;
}

export interface LinkedWikiPage {
  id: number;
  title: string;
  slug: string;
  category: string;
}

export interface LinkedTicketSummary {
  id: number;
  ticketKey: string;
  title: string;
  status: string;
  priority: string;
}

export interface TeamRule {
  id: number;
  team: number | null;
  teamName: string | null;
  title: string;
  content: string;
  category: string;
  sort_order: number;
  is_active: boolean;
  createdBy: UserSummary | null;
  createdAt: string;
  updatedAt: string;
}

export interface TeamRuleSummary {
  id: number;
  title: string;
  category: string;
  teamName: string | null;
}

// --- AI 分析 ---

export interface RuleViolation {
  rule_title: string;
  warning: string;
}

export interface ContextAnalysisResult {
  context_loaded: boolean;
  rule_violations: RuleViolation[];
  implementation_hint: string;
}

export interface WikiDraft {
  category: string;
  title: string;
  content_markdown: string;
}

export interface SuggestedBacklogTicket {
  title: string;
  description: string;
}

export interface CloseAnalysisResult {
  has_future_challenges: boolean;
  wiki_draft: WikiDraft | null;
  suggested_backlog_tickets: SuggestedBacklogTicket[];
}
