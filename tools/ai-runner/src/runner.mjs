#!/usr/bin/env node
// runner.mjs — SENN の「AIに対応させる」を実行するランナー(社内マシン・OSS 利用者のマシンで動かす)。
// 依頼を取得 → 検証 → 着手報告 → サンドボックスで AI → ハートビート → 点検・コミット → 完了/失敗の報告。
// 着手・完了・失敗の報告と使用量の記録は、この外側のプログラムが行う(AI は SENN の鍵を持たない)。
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseConfig, isWithinActiveHours } from './config.mjs';
import { SennClient, withRetry } from './client.mjs';
import { verifyJob } from './verify.mjs';
import { UsageMeter } from './meter.mjs';
import { prepareWorkspace, runSandbox, finalizeWork, assertIsolatedNetwork } from './executor.mjs';
import { redact } from './secrets.mjs';

const sleepDefault = (ms) => new Promise((r) => setTimeout(r, ms));

export class Runner {
  constructor({ config, client, deps = {}, log = (m) => console.log(`${new Date().toISOString()} ${m}`), sleep = sleepDefault }) {
    this.config = config;
    this.client = client;
    this.log = log;
    this.sleep = sleep;
    this.deps = { prepareWorkspace, runSandbox, finalizeWork, ...deps };
    this.shuttingDown = false;
    this.current = null; // 実行中の { stopReason }
  }

  /** いま依頼を受け付けてよいか(理由つき)。 */
  acceptance() {
    if (this.shuttingDown) return { ok: false, why: '終了処理中' };
    if (fs.existsSync(this.config.stopFile)) return { ok: false, why: `STOP ファイルがあります(${this.config.stopFile})` };
    if (!isWithinActiveHours(this.config.activeHours)) return { ok: false, why: '実行してよい時間帯の外です' };
    return { ok: true };
  }

  /** 1 回分: 依頼を取得して、あれば最後まで処理する。処理した(または試みた)なら true。 */
  async tick() {
    const acc = this.acceptance();
    const res = await withRetry(() => this.client.claim(acc.ok));
    if (!res?.job) return false;
    await this.process(res.job);
    return true;
  }

  async report(fn) {
    try { await withRetry(fn); } catch (e) { this.log(`報告に失敗しました: ${e.message}`); }
  }

  async process(rawJob) {
    let job;
    try {
      job = verifyJob(rawJob, this.config);
    } catch (e) {
      this.log(`依頼を拒否しました: ${e.message}`);
      // 署名の不一致などは、SENN 側にも失敗として残す(実行はしない)
      if (typeof rawJob?.runUuid === 'string') {
        await this.report(() => this.client.error(rawJob.runUuid, { detail: `ランナーが依頼を拒否しました: ${e.message}`, usage: {} }));
      }
      return;
    }
    const meter = new UsageMeter();
    const state = { stopReason: null };
    this.current = state;
    const usage = () => meter.usage();
    let dir = null;
    try {
      const started = await withRetry(() => this.client.start(job.runUuid, this.config.model || undefined));
      if (started?.stop) {
        state.stopReason = started.reason ?? 'stopped';
        await this.report(() => this.client.finish(job.runUuid, { result: 'aborted', summary: `開始前に停止されました(${state.stopReason})`, usage: usage() }));
        return;
      }
      dir = path.join(this.config.workRoot, job.runUuid);
      const ws = await this.deps.prepareWorkspace(this.config, job, dir);

      // ハートビート: 使用量を送り、止める指示を受け取る
      let hbBusy = false;
      const hb = setInterval(async () => {
        if (hbBusy) return;
        hbBusy = true;
        try {
          const r = await this.client.heartbeat(job.runUuid, usage());
          if (r?.stop && !state.stopReason) state.stopReason = r.reason ?? 'stopped';
        } catch (e) {
          // 通信が切れたまま続けない(SENN が止める指示を出せないため)
          state.failures = (state.failures ?? 0) + 1;
          this.log(`ハートビート失敗(${state.failures}): ${e.message}`);
          if (state.failures >= 3 && !state.stopReason) state.stopReason = 'heartbeat_failed';
        } finally { hbBusy = false; }
      }, this.config.heartbeatMs);
      hb.unref?.();
      let run;
      try {
        run = await this.deps.runSandbox(this.config, job, ws.workspace, {
          meter,
          shouldStop: () => state.stopReason ?? (fs.existsSync(this.config.stopFile) ? 'killed' : null),
        });
      } finally { clearInterval(hb); }

      const stopReason = run.killedBy ?? state.stopReason;
      if (stopReason) {
        await this.report(() => this.client.finish(job.runUuid, { result: 'aborted', summary: `停止しました(${stopReason})`, usage: usage(), usdEquivalent: meter.usdEquivalent() }));
        return;
      }
      const r = meter.result;
      if (run.code !== 0 || !r || r.is_error) {
        const sub = r?.subtype === 'error_max_turns' ? 'ターン数の上限に達しました' : (run.stderr || r?.result || `終了コード ${run.code}`);
        await this.report(() => this.client.error(job.runUuid, { detail: redact(String(sub), this.secrets()), usage: usage() }));
        return;
      }
      const fin = await this.deps.finalizeWork(this.config, job, ws, this.log);
      // AI の最終報告はチケットのコメントになる(Guest も読める)。ランナーの秘密を必ず伏せる(符号化した形も)
      const text = redact((meter.resultText || '').slice(0, 1500), this.secrets());
      let result, summary;
      if (fin.blocked) { result = 'blocked'; summary = `${fin.blocked}\n${text}`; }
      else if (!fin.changed) { result = 'blocked'; summary = `変更がありませんでした。\n${text}`; }
      else { result = 'resolved'; summary = `${text}\n${fin.pushed ? `ブランチ ${fin.branch} を push しました` : fin.note}`; }
      await this.report(() => this.client.finish(job.runUuid, { result, summary, usage: usage(), usdEquivalent: meter.usdEquivalent() }));
      if (fin.pushed && dir) fs.rmSync(dir, { recursive: true, force: true });
    } catch (e) {
      this.log(`実行中にエラー: ${e.message}`);
      await this.report(() => this.client.error(job.runUuid, { detail: redact(`ランナーのエラー: ${e.message}`, this.secrets()), usage: usage() }));
    } finally {
      this.current = null;
    }
  }

  /** 報告・コミットから伏せる、ランナーの秘密。 */
  secrets() {
    return [this.config.oauth, this.config.githubToken, this.config.runnerToken, this.config.signingSecret, this.config.cfAccess?.secret];
  }

  async run() {
    this.log(`ランナーを開始しました(${this.config.senn}、安全モード=${this.config.safeMode})`);
    let failures = 0;
    while (!this.shuttingDown) {
      let worked = false;
      try {
        worked = await this.tick();
        failures = 0;
      } catch (e) {
        failures++;
        this.log(`SENN との通信に失敗しました(${failures}): ${e.message}`);
      }
      if (!worked) await this.sleep(Math.min(this.config.pollMs * 2 ** Math.min(failures, 5), 300_000));
    }
  }

  shutdown() {
    this.shuttingDown = true;
    if (this.current && !this.current.stopReason) this.current.stopReason = 'runner_shutdown';
  }
}

async function main() {
  let config;
  try { config = parseConfig(); } catch (e) { console.error(e.message); process.exit(2); }
  try { await assertIsolatedNetwork(config); } catch (e) { console.error(e.message); process.exit(2); }
  fs.mkdirSync(config.workRoot, { recursive: true, mode: 0o700 });
  const client = new SennClient({ baseUrl: config.senn, token: config.runnerToken, cfAccess: config.cfAccess });
  const runner = new Runner({ config, client });
  for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => { runner.log(`${sig} を受け取りました。終了します`); runner.shutdown(); });
  await runner.run();
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
