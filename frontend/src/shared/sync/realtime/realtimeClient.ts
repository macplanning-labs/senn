/**
 * realtimeClient.ts — WebSocket の接続・再接続・パケットの直列処理
 *
 * 設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §5.4, §8.2
 *
 * - 接続は代表タブだけが張る（Web Locks）。他のタブは、代表タブが端末内 DB に書いた変更を liveQuery で受け取る
 * - 接続用トークンは毎回取り直す（1回限り・30秒で切れる）。ブラウザの WebSocket は 401 の中身を読めないので、
 *   つなぎ直すたびに新しいトークンを使う。本当にログインが切れていればトークン発行 API が 401 になり、既存の再ログインへ進む
 * - 切れたら指数バックオフ＋揺らぎで再接続する（一斉再接続を避ける）
 * - 書き込みは送らない（WebSocket は受け取り専用）。書き込みは今までどおり REST（送信キュー）
 */

import { apiClient } from '../../api/client';
import { applyAllSignals } from './applySignals';
import { applyAccess, applyDelta, applyResync, applyWelcome, buildResume, type DeltaOutcome } from './applyDelta';
import { isServerPacket, type ServerPacket } from './packets';
import { setLocalConnected, startStatusSharing } from './realtimeStatus';

export interface RealtimeHandlers {
  /** 差分同期で埋めてほしいとき */
  onCatchUp: () => void;
}

const MAX_BACKOFF_MS = 30_000;
/** サーバーが無効（503）のとき。従来の定期同期だけで動くので、急いでつなぎ直さない */
const DISABLED_BACKOFF_MS = 5 * 60_000;

// ── 接続状態（syncEngine が定期同期の間隔を決めるのに使う） ──
let connected = false;
const statusListeners = new Set<(c: boolean) => void>();

export function isRealtimeConnected(): boolean {
  return connected;
}

export function onRealtimeStatus(cb: (c: boolean) => void): () => void {
  statusListeners.add(cb);
  return () => statusListeners.delete(cb);
}

function setConnected(next: boolean): void {
  if (connected === next) return;
  connected = next;
  setLocalConnected(next);
  statusListeners.forEach((cb) => cb(next));
}

/** WebSocket の URL。API の baseURL（相対でも絶対でも）から作る */
export function buildWsUrl(token: string, base: string = apiClient.defaults.baseURL ?? '/api/v1', origin: string = window.location.origin): string {
  const url = new URL(base, origin);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.pathname = `${url.pathname.replace(/\/$/, '')}/realtime/ws`;
  url.search = '';
  url.searchParams.set('token', token);
  return url.toString();
}

/** 再接続までの待ち時間（指数バックオフ＋揺らぎ） */
export function backoffMs(attempt: number, random: number = Math.random()): number {
  const base = Math.min(MAX_BACKOFF_MS, 500 * 2 ** Math.max(0, attempt - 1));
  return Math.round(base * (0.5 + random));
}

interface Ctl {
  stopped: boolean;
  ws: WebSocket | null;
  wake: (() => void) | null;
}

function sleep(ms: number, ctl: Ctl): Promise<void> {
  return new Promise((resolve) => {
    const t = setTimeout(resolve, ms);
    ctl.wake = () => {
      clearTimeout(t);
      resolve();
    };
  });
}

function statusOf(err: unknown): number | undefined {
  return (err as { response?: { status?: number } })?.response?.status;
}

/** リアルタイムを開始する。戻り値は停止関数 */
export function startRealtime(userId: number, handlers: RealtimeHandlers): () => void {
  if (typeof WebSocket === 'undefined' || typeof window === 'undefined') return () => undefined;
  const ctl: Ctl = { stopped: false, ws: null, wake: null };
  const stopStatusSharing = startStatusSharing(userId);

  const run = async () => {
    let attempt = 0;
    while (!ctl.stopped) {
      const startedAt = Date.now();
      let disabled = false;
      try {
        await connectOnce(ctl, handlers);
      } catch (err) {
        disabled = statusOf(err) === 503;
      }
      setConnected(false);
      if (ctl.stopped) break;
      // 長く続いた接続のあとは、すぐにつなぎ直す
      attempt = Date.now() - startedAt > 60_000 ? 1 : attempt + 1;
      await sleep(disabled ? DISABLED_BACKOFF_MS : backoffMs(attempt), ctl);
    }
  };

  const locks = typeof navigator !== 'undefined' ? navigator.locks : undefined;
  if (locks?.request) {
    void locks.request(`senn-realtime-${userId}`, () => (ctl.stopped ? undefined : run()));
  } else {
    void run();
  }

  return () => {
    ctl.stopped = true;
    ctl.ws?.close(1000);
    ctl.wake?.();
    setConnected(false);
    stopStatusSharing();
  };
}

async function connectOnce(ctl: Ctl, handlers: RealtimeHandlers): Promise<void> {
  const { data } = await apiClient.post<{ token: string }>('/realtime/connect-token/');
  if (ctl.stopped) return;
  const ws = new WebSocket(buildWsUrl(data.token));
  ctl.ws = ws;

  // パケットは届いた順に1つずつ処理する（IndexedDB の書き込みが前後しないように）
  let chain: Promise<void> = Promise.resolve();
  let epoch: string | null = null;

  const handle = async (p: ServerPacket): Promise<void> => {
    let outcome: DeltaOutcome | null = null;
    switch (p.type) {
      case 'welcome':
        epoch = p.epoch;
        await applyWelcome(p);
        setConnected(true);
        return;
      case 'delta':
        epoch = p.epoch;
        outcome = await applyDelta(p);
        break;
      case 'resync':
        epoch = p.epoch;
        outcome = await applyResync(p);
        break;
      case 'access':
        outcome = await applyAccess(p, epoch);
        break;
      case 'ping':
        ws.send(JSON.stringify({ type: 'pong', t: p.t }));
        return;
    }
    if (outcome?.needsCatchUp) {
      handlers.onCatchUp();
      // 取りこぼした合図があるかもしれない。表の無い対象は、何が変わったか分からないのでキャッシュを取り直す
      applyAllSignals();
    }
  };

  await new Promise<void>((resolve) => {
    ws.onopen = () => {
      // 最初に受信位置を伝える。サーバーは再送できる分を送り、できない部屋は resync を返す
      void buildResume().then((resume) => ws.send(JSON.stringify(resume)));
    };
    ws.onmessage = (ev) => {
      let parsed: unknown;
      try {
        parsed = JSON.parse(typeof ev.data === 'string' ? ev.data : '');
      } catch {
        return;
      }
      if (!isServerPacket(parsed)) return;
      chain = chain.then(() => handle(parsed as ServerPacket)).catch((e) => console.error('[realtime] packet failed', e));
    };
    ws.onclose = () => resolve();
    ws.onerror = () => undefined; // onclose が続いて呼ばれる
  });
  ctl.ws = null;
}
