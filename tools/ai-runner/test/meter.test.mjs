import test from 'node:test';
import assert from 'node:assert/strict';
import { UsageMeter, weightedTokens } from '../src/meter.mjs';

const asst = (id, u) => JSON.stringify({ type: 'assistant', message: { id, usage: { input_tokens: u[0], output_tokens: u[1], cache_creation_input_tokens: u[2], cache_read_input_tokens: u[3] } } });

test('加重トークンの式(キャッシュ読み取りは 1/10)', () => {
  assert.equal(weightedTokens({ inputTokens: 100, outputTokens: 50, cacheCreationTokens: 1000, cacheReadTokens: 10000 }), 100 + 50 + 1000 + 1000);
});
test('同じメッセージ ID は二重に数えない(最大値を採用)', () => {
  const m = new UsageMeter();
  m.feed(asst('a', [10, 1, 0, 0]));
  m.feed(asst('a', [10, 20, 0, 0]));
  m.feed(asst('b', [5, 5, 0, 0]));
  assert.equal(m.turns, 2);
  assert.deepEqual(m.usage(), { inputTokens: 15, outputTokens: 25, cacheCreationTokens: 0, cacheReadTokens: 0 });
});
test('壊れた行・空行・未知のイベントは無視する', () => {
  const m = new UsageMeter();
  m.feed('');
  m.feed('{broken');
  m.feed(JSON.stringify({ type: 'system' }));
  assert.equal(m.parseErrors, 1);
  assert.equal(m.weighted(), 0);
});
test('result 行から結果と金額を読む', () => {
  const m = new UsageMeter();
  m.feed(JSON.stringify({ type: 'result', result: '完了しました', total_cost_usd: 1.25, is_error: false }));
  assert.equal(m.resultText, '完了しました');
  assert.equal(m.usdEquivalent(), 1.25);
  const bad = new UsageMeter();
  bad.feed(JSON.stringify({ type: 'result', total_cost_usd: -1 }));
  assert.equal(bad.usdEquivalent(), undefined);
});
