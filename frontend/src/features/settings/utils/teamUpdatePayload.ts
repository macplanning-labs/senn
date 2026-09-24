/**
 * teamUpdatePayload.ts — チーム設定の更新リクエストの組み立て
 *
 * 「有効」(isActive)は画面から外したが、バックエンドは未指定の is_active を false として
 * 扱う。更新のたびにチームが無効化されないよう、現在の値をそのまま送る。
 */

import type { TeamFormData } from '@/features/teams/hooks/useTeams';

interface Input {
  name: string;
  description: string;
  icon: string;
  color: string;
  slackWebhookUrl: string;
  prefix: string;
  /** 現在のチームの「有効」(画面では変更しないので、そのまま送る) */
  isActive: boolean;
}

export function buildTeamUpdateData(input: Input): Partial<TeamFormData> {
  const trimmedPrefix = input.prefix.trim().toUpperCase();
  return {
    name: input.name.trim(),
    description: input.description.trim(),
    icon: input.icon,
    color: input.color,
    slackWebhookUrl: input.slackWebhookUrl.trim() || undefined,
    isActive: input.isActive,
    ...(trimmedPrefix ? { prefix: trimmedPrefix } : {}),
  };
}
