export interface Comment {
  id: number;
  body: string;
  author: { id: number; username: string; displayName: string };
  /**
   * AIエージェント経由(人ごとキー認証成功時)の実際の実行者。
   * 設定されている場合、投稿者表示は author ではなくこちらを主表示にする(WIPAPPDEV-000100)。
   */
  actingUser?: { id: number; username: string; displayName: string } | null;
  createdAt: string;
  updatedAt: string | null;
  isDeleted: boolean;
  parentCommentId: number | null;
  replyCount: number;
  canEdit: boolean;
  canDelete: boolean;
}

export interface ReferenceLink {
  id: number;
  url: string;
  title: string | null;
  createdBy: { id: number; displayName: string };
  createdAt: string;
}

export interface TicketAttachment {
  id: number;
  filename: string;
  fileSize: number;
  sizeDisplay: string;
  isImage: boolean;
  createdAt: string;
  uploader: { id: number; username: string; displayName: string };
  fileUrl: string;
  commentId?: number | null;
}

/** API TicketDetailOut に合わせた詳細画面用型（createdBy は使わない） */
export interface TicketDetailView {
  id: number;
  ticketKey: string;
  title: string;
  description: string;
  ticketType?: string | null;
  status: string;
  priority: string;
  assignees: Array<{ id: number; username: string; displayName: string }>;
  reviewers: Array<{ id: number; username: string; displayName: string }>;
  author: { id: number; username: string; displayName: string } | null;
  category: { id: number; name: string; color: string } | null;
  milestone: { id: number; name: string; dueDate: string | null } | null;
  project: number | null;
  projectPrefix?: string | null;
  projectName?: string | null;
  team?: { id: number; slug: string; name: string } | null;
  startDate: string | null;
  dueDate: string | null;
  createdAt: string;
  updatedAt: string;
  storyPoints: number | null;
  cycle: number | null;
  cycleName: string | null;
  labels: Array<{ id: number; name: string; color: string }>;
  linkedWikiPages?: Array<{ id: number; title: string; slug: string; category: string }>;
  isWatching: boolean;
  childCount?: number;
  comments?: Comment[];
  links?: ReferenceLink[];
  attachments?: TicketAttachment[];
}
