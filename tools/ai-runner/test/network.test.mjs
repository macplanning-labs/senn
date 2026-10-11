import test from 'node:test';
import assert from 'node:assert/strict';
import { assertIsolatedNetwork } from '../src/executor.mjs';

const cfg = { sandboxNetwork: 'senn-ai-internal', egressNetwork: 'senn-ai-egress', dockerBin: 'docker' };
// docker network inspect の応答を、ネットワーク名ごとに返す偽物
const fakeExec = (answers) => async (bin, args) => {
  const name = args.at(-1);
  if (!(name in answers)) throw new Error('No such network');
  return { stdout: `${answers[name]}\n` };
};

test('内部ネットワークで、中継役と AI のコンテナだけなら起動してよい', async () => {
  await assertIsolatedNetwork(cfg, fakeExec({ 'senn-ai-internal': 'true|senn-ai-gw-123e4567 senn-ai-123e4567 ', 'senn-ai-egress': 'senn-ai-egress' }));
  await assertIsolatedNetwork(cfg, fakeExec({ 'senn-ai-internal': 'true|', 'senn-ai-egress': 'senn-ai-egress' }));
});

test('外に出られるネットワーク(bridge など)では起動しない', async () => {
  await assert.rejects(() => assertIsolatedNetwork(cfg, fakeExec({ 'senn-ai-internal': 'false|', 'senn-ai-egress': 'x' })), /internal: true/);
});

test('以前の版のプロキシなど、ほかのコンテナがつながっていたら起動しない(中継役を通らずに外へ出られるため)', async () => {
  await assert.rejects(
    () => assertIsolatedNetwork(cfg, fakeExec({ 'senn-ai-internal': 'true|senn-ai-proxy senn-ai-gw-123e4567 ', 'senn-ai-egress': 'x' })),
    /senn-ai-proxy/,
  );
});

test('ネットワークが無ければ、作り方を案内して起動しない', async () => {
  await assert.rejects(() => assertIsolatedNetwork(cfg, fakeExec({})), /setup-networks\.sh/);
  await assert.rejects(() => assertIsolatedNetwork(cfg, fakeExec({ 'senn-ai-internal': 'true|' })), /senn-ai-egress/);
});
