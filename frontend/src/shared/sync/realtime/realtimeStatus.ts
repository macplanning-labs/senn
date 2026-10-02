/**
 * realtimeStatus.ts — リアルタイム接続状態の管理とタブ間共有
 *
 * - このタブの接続状態（realtimeClient が設定）
 * - 他のタブの接続状態（BroadcastChannel で共有）
 * - 有効状態（local || remote）を subscribe
 */

export type RealtimeState = 'connected' | 'disconnected';

interface Message {
  type: 'hello' | 'status';
  connected?: boolean;
}

/** 代表タブが、接続中は状態を知らせる間隔 */
const HEARTBEAT_MS = 10_000;
/** 他のタブから知らせが届かないまま、これだけ経ったら「切れた」とみなす。
 * 代表タブが強制終了・クラッシュすると切断の連絡が届かないため（表示が古い「接続中」のまま残るのを防ぐ） */
const REMOTE_TTL_MS = 30_000;

let localConnected = false;
let remoteConnected = false;
let channel: BroadcastChannel | null = null;
let heartbeat: ReturnType<typeof setInterval> | null = null;
let remoteExpiry: ReturnType<typeof setTimeout> | null = null;

const listeners = new Set<(state: RealtimeState) => void>();
let lastState: RealtimeState = getEffectiveStateInternal();

/**
 * このタブが代表タブとして自分で接続しているか（realtimeClient が設定）
 */
export function setLocalConnected(v: boolean): void {
  if (localConnected === v) return;
  localConnected = v;
  if (channel) {
    channel.postMessage({ type: 'status', connected: v } satisfies Message);
  }
  syncHeartbeat();
  notifyIfChanged();
}

/** 接続中で、共有の場があるときだけ、定期的に状態を知らせる */
function syncHeartbeat(): void {
  if (heartbeat) {
    clearInterval(heartbeat);
    heartbeat = null;
  }
  if (localConnected && channel) {
    heartbeat = setInterval(() => channel?.postMessage({ type: 'status', connected: true } satisfies Message), HEARTBEAT_MS);
  }
}

function clearRemoteExpiry(): void {
  if (remoteExpiry) {
    clearTimeout(remoteExpiry);
    remoteExpiry = null;
  }
}

function getEffectiveStateInternal(): RealtimeState {
  return localConnected || remoteConnected ? 'connected' : 'disconnected';
}

/**
 * 有効状態（local || remote）
 */
export function getEffectiveState(): RealtimeState {
  return getEffectiveStateInternal();
}

function notifyIfChanged(): void {
  const newState = getEffectiveStateInternal();
  if (newState !== lastState) {
    lastState = newState;
    listeners.forEach((cb) => cb(newState));
  }
}

/**
 * 有効状態の変更を subscribe。リッスナーは状態が変わったときだけ呼ばれる
 */
export function subscribeRealtimeState(cb: (s: RealtimeState) => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

/**
 * BroadcastChannel で接続状態を共有。戻り値は停止関数
 */
export function startStatusSharing(newUserId: number): () => void {
  if (typeof BroadcastChannel === 'undefined') {
    return () => undefined;
  }

  const channelName = `senn-realtime-status-${newUserId}`;
  channel = new BroadcastChannel(channelName);

  const handleMessage = (event: MessageEvent<unknown>): void => {
    const msg = event.data as Message;
    if (msg.type === 'hello') {
      // 他のタブが hello を送ってきた。自分が接続していれば status で返す
      if (localConnected && channel) {
        channel.postMessage({ type: 'status', connected: true } satisfies Message);
      }
    } else if (msg.type === 'status') {
      // 他のタブから接続状態を受け取る
      remoteConnected = msg.connected ?? false;
      clearRemoteExpiry();
      if (remoteConnected) {
        // 次の知らせが来なければ、代表タブはいなくなったとみなす
        remoteExpiry = setTimeout(() => {
          remoteExpiry = null;
          remoteConnected = false;
          notifyIfChanged();
        }, REMOTE_TTL_MS);
      }
      notifyIfChanged();
    }
  };

  channel.addEventListener('message', handleMessage);

  // 新規参加タブとして hello を送り、既に接続しているタブから status をもらう
  channel.postMessage({ type: 'hello' } satisfies Message);
  syncHeartbeat(); // すでに接続中なら、定期の知らせを始める

  return () => {
    if (channel && channel.name === channelName) {
      channel.removeEventListener('message', handleMessage);
      channel.close();
      channel = null;
      remoteConnected = false;
      clearRemoteExpiry();
      syncHeartbeat();
      notifyIfChanged();
    }
  };
}

/**
 * テスト用: モジュール状態をリセット
 */
export function resetForTests(): void {
  localConnected = false;
  remoteConnected = false;
  clearRemoteExpiry();
  if (heartbeat) {
    clearInterval(heartbeat);
    heartbeat = null;
  }
  if (channel) {
    channel.close();
    channel = null;
  }
  listeners.clear();
  lastState = 'disconnected';
}
