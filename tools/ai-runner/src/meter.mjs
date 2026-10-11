// meter.mjs — `claude -p --output-format stream-json --verbose` の出力から、使用量を数える。
// 加重トークン = 入力 + 出力 + キャッシュ作成 + キャッシュ読み取り × 0.1(サーバーと同じ式)。
// 同じメッセージ ID が複数行に出ることがあるため、ID ごとに最大値を採用して二重に数えない。

export function weightedTokens(u) {
  return Math.floor(u.inputTokens + u.outputTokens + u.cacheCreationTokens + u.cacheReadTokens * 0.1);
}

export class UsageMeter {
  constructor() {
    this.byMessage = new Map();
    this.turns = 0;
    this.result = null; // 最後の result 行
    this.resultText = '';
    this.parseErrors = 0;
  }

  /** 1 行(JSON)を取り込む。壊れた行は数えて無視する。 */
  feed(line) {
    const s = line.trim();
    if (!s) return;
    let ev;
    try { ev = JSON.parse(s); } catch { this.parseErrors++; return; }
    if (ev.type === 'assistant' && ev.message) {
      const u = ev.message.usage;
      const id = ev.message.id ?? `anon-${this.byMessage.size}`;
      if (u) {
        const cur = {
          inputTokens: u.input_tokens ?? 0,
          outputTokens: u.output_tokens ?? 0,
          cacheCreationTokens: u.cache_creation_input_tokens ?? 0,
          cacheReadTokens: u.cache_read_input_tokens ?? 0,
        };
        const prev = this.byMessage.get(id);
        if (!prev) {
          this.turns++;
          this.byMessage.set(id, cur);
        } else {
          for (const k of Object.keys(cur)) prev[k] = Math.max(prev[k], cur[k]);
        }
      }
    } else if (ev.type === 'result') {
      this.result = ev;
      this.resultText = typeof ev.result === 'string' ? ev.result : '';
    }
  }

  usage() {
    const t = { inputTokens: 0, outputTokens: 0, cacheCreationTokens: 0, cacheReadTokens: 0 };
    for (const u of this.byMessage.values()) for (const k of Object.keys(t)) t[k] += u[k];
    return t;
  }

  weighted() { return weightedTokens(this.usage()); }

  usdEquivalent() {
    const v = this.result?.total_cost_usd;
    return typeof v === 'number' && Number.isFinite(v) && v >= 0 ? v : undefined;
  }
}
