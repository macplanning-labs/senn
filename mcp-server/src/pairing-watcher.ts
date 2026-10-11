// 端末の接続の「待ち」を見張る(DEMO-000192 フェーズ4)
// 承認を待つ間、30秒ごとにサーバーへ状態を聞き、承認されたら自動でキーを保存・入れ替える。
// MCP の起動時に、待ちの状態が残っていれば再開する。標準出力はMCPの通信に使うので、ログは stderr にだけ出す。
import { WipClient, WipApiError } from "./client.js";
import {
  applyStatus,
  clearState,
  defaultDir,
  loadState,
  type Outcome,
  type PairingState,
  type RemoteStatus,
} from "./pairing.js";

const POLL_MS = 30_000;

export type LastResult =
  | { kind: "none" }
  | { kind: "connected" }
  | { kind: "closed"; status: "rejected" | "failed" | "expired"; message: string | null };

export class PairingWatcher {
  private timer: NodeJS.Timeout | null = null;
  lastResult: LastResult = { kind: "none" };
  lastStatus: RemoteStatus | null = null;

  constructor(
    private client: WipClient,
    private dir: string = defaultDir()
  ) {}

  /** 待ちの状態があれば、見張りを(再)開始する */
  resume(): PairingState | null {
    const st = loadState(this.dir);
    if (!st) return null;
    this.startTimer();
    void this.checkOnce();
    return st;
  }

  current(): PairingState | null {
    return loadState(this.dir);
  }

  /** 新しい接続を始めたあと、見張りを開始する */
  watch(): void {
    this.startTimer();
  }

  private startTimer(): void {
    if (this.timer) return;
    this.timer = setInterval(() => void this.checkOnce(), POLL_MS);
    // 見張りだけでは、プロセスを終了させない
    this.timer.unref();
  }

  private stopTimer(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  /** 1回だけ状態を聞いて、反映する。結果(待ち・接続・終了)を返す。待ちの状態が無ければ null */
  async checkOnce(): Promise<Outcome | null> {
    const st = loadState(this.dir);
    if (!st) {
      this.stopTimer();
      return null;
    }
    try {
      const reply = await this.client.pairingStatus(st.code, st.deviceSecret);
      this.lastStatus = reply.status as RemoteStatus;
      const outcome = applyStatus(
        this.dir,
        st,
        { status: reply.status as RemoteStatus, message: reply.message },
        (k) => this.client.setApiKey(k),
        new Date(),
        this.client.apiBaseUrl
      );
      if (outcome.kind === "connected") {
        this.lastResult = { kind: "connected" };
        this.stopTimer();
        console.error("SENN: AI Agent の接続が承認され、この Mac のキーを保存しました");
      } else if (outcome.kind === "closed") {
        this.lastResult = { kind: "closed", status: outcome.status, message: outcome.message };
        this.stopTimer();
        console.error(`SENN: AI Agent の接続は終了しました(${outcome.status})`);
      }
      return outcome;
    } catch (e) {
      if (e instanceof WipApiError && e.status === 404) {
        // サーバーが、この確認コードを知らない(期限切れで消えた、など)。待ちを捨てる
        clearState(this.dir);
        this.lastResult = { kind: "closed", status: "expired", message: null };
        this.stopTimer();
        return { kind: "closed", status: "expired", message: null };
      }
      // 通信の失敗は、次の回に再試行する(キーは捨てない)
      console.error("SENN: 接続の状態の確認に失敗しました。あとで再試行します");
      return null;
    }
  }
}
