// verify.mjs — SENN から受け取った依頼を、実行する前に検証する。
// 署名・プロンプトのハッシュ・各項目の形式・リポジトリの許可・上限(自分の絶対上限との小さいほう)。
import crypto from 'node:crypto';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const KEY_RE = /^[A-Z][A-Z0-9]{0,15}-\d{1,9}$/;
const REPO_RE = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;
const BRANCH_RE = /^[A-Za-z0-9][A-Za-z0-9._/-]{0,99}$/;

export function sha256Hex(s) {
  return crypto.createHash('sha256').update(s, 'utf8').digest('hex');
}

/** サーバー(Rust の signing_string)と同じ文字列。 */
export function signingString(job) {
  const c = job.caps;
  return `${job.runUuid}\n${job.ticketKey}\n${job.promptHash}\n${job.repo}\n${job.baseBranch}\n${c.maxTurns}\n${c.maxMinutes}\n${c.maxWeightedTokens}`;
}

export function sign(secret, message) {
  return crypto.createHmac('sha256', secret).update(message, 'utf8').digest('hex');
}

function safeEqual(a, b) {
  const x = Buffer.from(String(a));
  const y = Buffer.from(String(b));
  return x.length === y.length && crypto.timingSafeEqual(x, y);
}

function invalidBranch(b) {
  return !BRANCH_RE.test(b) || b.includes('..') || b.endsWith('/') || b.endsWith('.lock') || b.includes('//');
}

/**
 * 検証して、実行に使う値(上限を丸めたもの)を返す。問題があれば Error(message は日本語)。
 * @returns {{runUuid, ticketKey, prompt, repo, baseBranch, caps:{maxTurns,maxMinutes,maxWeightedTokens}}}
 */
export function verifyJob(job, config) {
  if (!job || typeof job !== 'object') throw new Error('依頼の形式が不正です');
  const { runUuid, ticketKey, prompt, promptHash, repo, baseBranch, caps, signature } = job;
  if (typeof runUuid !== 'string' || !UUID_RE.test(runUuid)) throw new Error('runUuid の形式が不正です');
  if (typeof ticketKey !== 'string' || !KEY_RE.test(ticketKey)) throw new Error('ticketKey の形式が不正です');
  if (typeof repo !== 'string' || !REPO_RE.test(repo)) throw new Error('repo の形式が不正です');
  if (typeof baseBranch !== 'string' || invalidBranch(baseBranch)) throw new Error('baseBranch の形式が不正です');
  if (typeof prompt !== 'string' || prompt.length === 0 || prompt.length > 200_000) throw new Error('prompt が不正です');
  if (!caps || ![caps.maxTurns, caps.maxMinutes, caps.maxWeightedTokens].every((n) => Number.isInteger(n) && n > 0)) {
    throw new Error('上限(caps)が不正です');
  }
  // 署名: 内容が途中で書き換えられていないこと
  if (typeof signature !== 'string' || !safeEqual(sign(config.signingSecret, signingString(job)), signature)) {
    throw new Error('署名が一致しません(依頼が改ざんされた可能性があります)');
  }
  if (typeof promptHash !== 'string' || !safeEqual(sha256Hex(prompt), promptHash)) {
    throw new Error('プロンプトのハッシュが一致しません');
  }
  if (!config.allowedRepos.includes(repo)) throw new Error(`許可されていないリポジトリです: ${repo}`);
  return {
    runUuid, ticketKey, prompt, repo, baseBranch,
    caps: {
      maxTurns: Math.min(caps.maxTurns, config.ceilings.maxTurns),
      maxMinutes: Math.min(caps.maxMinutes, config.ceilings.maxMinutes),
      maxWeightedTokens: Math.min(caps.maxWeightedTokens, config.ceilings.maxWeightedTokens),
    },
  };
}

/** 作業用ブランチ名(SENN のプロンプトが AI に伝える名前と同じ。ticketKey は検証済みの形式)。 */
export function workBranch(job) {
  return `ai/${job.ticketKey.toLowerCase()}`;
}
