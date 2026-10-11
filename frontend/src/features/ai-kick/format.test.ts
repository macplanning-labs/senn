/**
 * ai-kick/format.test.ts - AI Kick フォーマット関数のテスト
 */

import { describe, it, expect } from 'vitest';
import {
  formatTokens,
  statusLabelKey,
  isActive,
  priorityBadgeClass,
  canShowAiKick,
  runResultLabelKey,
  stopReasonLabelKey,
} from './format';
import type { AiKickRun } from './api';

function makeRun(overrides: Partial<AiKickRun> = {}): AiKickRun {
  return {
    runUuid: 'test-uuid',
    status: 'queued',
    triggerType: 'manual',
    priorityClass: 'normal',
    queuedAt: '2026-10-07T12:00:00Z',
    startedAt: null,
    endedAt: null,
    result: null,
    summary: null,
    weightedTokens: 0,
    stopReason: null,
    errorDetail: null,
    ...overrides,
  };
}

describe('formatTokens', () => {
  it('100万以上は "M" で表示', () => {
    expect(formatTokens(1_234_567)).toBe('1.2M');
    expect(formatTokens(1_000_000)).toBe('1M');
    expect(formatTokens(5_678_900)).toBe('5.7M');
  });

  it('1000以上100万未満は "k" で表示', () => {
    expect(formatTokens(12_345)).toBe('12k');
    expect(formatTokens(1_000)).toBe('1k');
    expect(formatTokens(99_999)).toBe('100k');
  });

  it('1000未満は数字のみ', () => {
    expect(formatTokens(999)).toBe('999');
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(1)).toBe('1');
  });

  it('端数を正しく丸める', () => {
    expect(formatTokens(1_049_000)).toBe('1M'); // 1.049 → 1.0
    expect(formatTokens(1_200_000)).toBe('1.2M'); // 1.2 → 1.2
    expect(formatTokens(9_999_999)).toBe('10M'); // 9.999999 → 10.0
  });
});

describe('statusLabelKey', () => {
  it('queued なら aiKick.status.queued', () => {
    expect(statusLabelKey(makeRun({ status: 'queued' }))).toBe('aiKick.status.queued');
  });

  it('running なら aiKick.status.running', () => {
    expect(statusLabelKey(makeRun({ status: 'running' }))).toBe('aiKick.status.running');
  });

  it('stopping なら aiKick.status.stopping', () => {
    expect(statusLabelKey(makeRun({ status: 'stopping' }))).toBe('aiKick.status.stopping');
  });

  it('finished で result=resolved なら aiKick.status.resolved', () => {
    expect(statusLabelKey(makeRun({ status: 'finished', result: 'resolved' }))).toBe(
      'aiKick.status.resolved'
    );
  });

  it('finished で result=blocked なら aiKick.status.blocked', () => {
    expect(statusLabelKey(makeRun({ status: 'finished', result: 'blocked' }))).toBe(
      'aiKick.status.blocked'
    );
  });

  it('finished で result=aborted なら aiKick.status.aborted', () => {
    expect(statusLabelKey(makeRun({ status: 'finished', result: 'aborted' }))).toBe(
      'aiKick.status.aborted'
    );
  });

  it('finished で result=error なら aiKick.status.error', () => {
    expect(statusLabelKey(makeRun({ status: 'finished', result: 'error' }))).toBe(
      'aiKick.status.error'
    );
  });

  it('finished で result=null ならunknown', () => {
    expect(statusLabelKey(makeRun({ status: 'finished', result: null }))).toBe(
      'aiKick.status.unknown'
    );
  });
});

describe('isActive', () => {
  it('queued はアクティブ', () => {
    expect(isActive(makeRun({ status: 'queued' }))).toBe(true);
  });

  it('running はアクティブ', () => {
    expect(isActive(makeRun({ status: 'running' }))).toBe(true);
  });

  it('stopping はアクティブ', () => {
    expect(isActive(makeRun({ status: 'stopping' }))).toBe(true);
  });

  it('finished は非アクティブ', () => {
    expect(isActive(makeRun({ status: 'finished' }))).toBe(false);
  });
});

describe('priorityBadgeClass', () => {
  it('emergency クラスを返す', () => {
    expect(priorityBadgeClass('emergency')).toContain('emergency');
  });

  it('high クラスを返す', () => {
    expect(priorityBadgeClass('high')).toContain('high');
  });

  it('normal クラスを返す', () => {
    expect(priorityBadgeClass('normal')).toContain('normal');
  });

  it('low クラスを返す', () => {
    expect(priorityBadgeClass('low')).toContain('low');
  });

  it('未知の優先度はデフォルトクラス', () => {
    expect(priorityBadgeClass('unknown')).toBe('ai-kick__priority-badge');
  });
});

describe('canShowAiKick', () => {
  it('ゲストには出さない', () => {
    expect(
      canShowAiKick({
        projectKick: { enabled: true, systemEnabled: true },
        isGuest: true,
      })
    ).toBe(false);
  });

  it('enabled=false では出さない', () => {
    expect(
      canShowAiKick({
        projectKick: { enabled: false, systemEnabled: true },
        isGuest: false,
      })
    ).toBe(false);
  });

  it('systemEnabled=false では出さない', () => {
    expect(
      canShowAiKick({
        projectKick: { enabled: true, systemEnabled: false },
        isGuest: false,
      })
    ).toBe(false);
  });

  it('全て条件を満たすと出す', () => {
    expect(
      canShowAiKick({
        projectKick: { enabled: true, systemEnabled: true, systemKilled: false },
        isGuest: false,
      })
    ).toBe(true);
  });

  it('projectKick がない場合は出さない', () => {
    expect(
      canShowAiKick({
        isGuest: false,
      })
    ).toBe(false);
  });
});

describe('runResultLabelKey', () => {
  it('resolved の場合', () => {
    expect(runResultLabelKey(makeRun({ status: 'finished', result: 'resolved' }))).toBe(
      'aiKick.result.resolved'
    );
  });

  it('blocked の場合', () => {
    expect(runResultLabelKey(makeRun({ status: 'finished', result: 'blocked' }))).toBe(
      'aiKick.result.blocked'
    );
  });

  it('aborted の場合', () => {
    expect(runResultLabelKey(makeRun({ status: 'finished', result: 'aborted' }))).toBe(
      'aiKick.result.aborted'
    );
  });

  it('error の場合', () => {
    expect(runResultLabelKey(makeRun({ status: 'finished', result: 'error' }))).toBe(
      'aiKick.result.error'
    );
  });

  it('null の場合は unknown', () => {
    expect(runResultLabelKey(makeRun({ result: null }))).toBe('aiKick.result.unknown');
  });
});

describe('stopReasonLabelKey', () => {
  it('null または undefined は空文字', () => {
    expect(stopReasonLabelKey(null)).toBe('');
    expect(stopReasonLabelKey(undefined)).toBe('');
  });

  it('per_run_tokens を返す', () => {
    expect(stopReasonLabelKey('per_run_tokens')).toBe('aiKick.stopReason.perRunTokens');
  });

  it('per_run_time を返す', () => {
    expect(stopReasonLabelKey('per_run_time')).toBe('aiKick.stopReason.perRunTime');
  });

  it('burn_rate を返す', () => {
    expect(stopReasonLabelKey('burn_rate')).toBe('aiKick.stopReason.burnRate');
  });

  it('killed を返す', () => {
    expect(stopReasonLabelKey('killed')).toBe('aiKick.stopReason.killed');
  });

  it('user_cancel を返す', () => {
    expect(stopReasonLabelKey('user_cancel')).toBe('aiKick.stopReason.userCancel');
  });

  it('budget_ceiling を返す', () => {
    expect(stopReasonLabelKey('budget_ceiling')).toBe('aiKick.stopReason.budgetCeiling');
  });

  it('ランナー系（heartbeat_lost, unclaimed, expired）を統一', () => {
    expect(stopReasonLabelKey('heartbeat_lost')).toBe('aiKick.stopReason.runnerLost');
    expect(stopReasonLabelKey('unclaimed')).toBe('aiKick.stopReason.runnerLost');
    expect(stopReasonLabelKey('expired')).toBe('aiKick.stopReason.runnerLost');
  });

  it('project_disabled を返す', () => {
    expect(stopReasonLabelKey('project_disabled')).toBe('aiKick.stopReason.projectDisabled');
  });

  it('runner_revoked を返す', () => {
    expect(stopReasonLabelKey('runner_revoked')).toBe('aiKick.stopReason.runnerRevoked');
  });

  it('未知の理由は unknown', () => {
    expect(stopReasonLabelKey('unknown_reason')).toBe('aiKick.stopReason.unknown');
  });
});
