/** AI Agent を利用した MCP 操作の申請(DEMO-000192)の型 */

export interface UserSummary {
  id: number;
  username: string;
  email: string;
  displayName: string;
}

/** expired は、承認済みで期限が切れたものをサーバーが計算して返す */
export type AiMcpRequestStatus =
  | 'pending'
  | 'approved'
  | 'rejected'
  | 'issued'
  | 'cancelled'
  | 'expired';

export interface AiMcpRequest {
  id: number;
  user: UserSummary;
  purpose: string;
  status: AiMcpRequestStatus;
  decidedBy: UserSummary | null;
  decidedAt: string | null;
  decisionNote: string;
  approvalExpiresAt: string | null;
  issuedAt: string | null;
  createdAt: string;
}

export interface AiMcpMyKey {
  id: number;
  keyPrefix: string;
  label: string | null;
  createdAt: string;
  lastUsedAt: string | null;
  revokedAt: string | null;
}

export interface AiMcpMine {
  requests: AiMcpRequest[];
  keys: AiMcpMyKey[];
  maxActiveKeys: number;
  approvalValidDays: number;
}

export interface AiMcpAdminList {
  requests: AiMcpRequest[];
  pendingCount: number;
}

export interface IssuedKey {
  id: number;
  plainKey: string;
  keyPrefix: string;
}

/** サーバーのエラー文(error / detail)を取り出す。無ければ fallback */
export function apiErrorMessage(e: unknown, fallback: string): string {
  const data = (e as { response?: { data?: { error?: unknown; detail?: unknown } } })?.response?.data;
  if (typeof data?.error === 'string') return data.error;
  if (typeof data?.detail === 'string') return data.detail;
  return fallback;
}

export const AI_MCP_MINE_KEY = ['ai-mcp', 'mine'] as const;
export const AI_MCP_ADMIN_KEY = ['ai-mcp', 'admin'] as const;
export const AI_MCP_PENDING_COUNT_KEY = ['ai-mcp', 'pending-count'] as const;
