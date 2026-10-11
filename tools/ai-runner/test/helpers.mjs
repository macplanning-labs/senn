import { parseConfig } from '../src/config.mjs';
import { sha256Hex, sign, signingString } from '../src/verify.mjs';

export const baseEnv = {
  SENN_URL: 'https://senn.example.com',
  RUNNER_TOKEN: 'srun_' + 'a'.repeat(43),
  RUNNER_SIGNING_SECRET: 'secret-for-tests',
  CLAUDE_CODE_OAUTH_TOKEN: 'sk-ant-oat01-test-token-value',
  ALLOWED_REPOS: 'acme/app',
  RUNNER_HOME: '/tmp/senn-ai-runner-test',
};
export const cfg = (extra = {}) => parseConfig({ ...baseEnv, ...extra });

export function makeJob(config, over = {}) {
  const prompt = over.prompt ?? 'チケットに対応してください';
  const job = {
    runUuid: '123e4567-e89b-42d3-a456-426614174000',
    ticketKey: 'SENN-12',
    prompt,
    promptHash: sha256Hex(prompt),
    repo: 'acme/app',
    baseBranch: 'develop',
    caps: { maxTurns: 100, maxMinutes: 30, maxWeightedTokens: 5_000_000 },
    ...over,
  };
  job.signature = over.signature ?? sign(config.signingSecret, signingString(job));
  return job;
}
