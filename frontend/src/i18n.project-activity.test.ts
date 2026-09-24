/**
 * Activity / 進捗報告の文言が EN/JA で過不足なく揃っていること、
 * および describeActivity が返すキーが全て存在することを検証する。
 */
import { describe, it, expect } from 'vitest';
import i18n from './i18n';

type Tree = { [k: string]: string | Tree };
const res = (lng: 'en' | 'ja') => (i18n.options.resources?.[lng]?.translation ?? {}) as Tree;
const flatten = (o: Tree, prefix = ''): string[] =>
  Object.entries(o).flatMap(([k, v]) => (typeof v === 'string' ? [prefix + k] : flatten(v, `${prefix}${k}.`)));

describe('project activity i18n', () => {
  for (const block of ['projectActivity', 'projectUpdates', 'projectTeams', 'teamArchive', 'teamsMenu'] as const) {
    it(`${block} のキーが EN と JA で一致する`, () => {
      const en = flatten(res('en')[block] as Tree).sort();
      const ja = flatten(res('ja')[block] as Tree).sort();
      expect(en.length).toBeGreaterThan(0);
      expect(ja).toEqual(en);
    });
  }

  it('describeActivity が返し得る全てのキーが定義されている', () => {
    const keys = [
      'projectCreated', 'changeName', 'changeDescription', 'changeStatus', 'changePriority',
      'changeTargetDate', 'changeTargetDateSet', 'changeTargetDateCleared', 'changeOwner', 'changeOther',
      'teamAdded', 'teamRemoved', 'milestoneCreated', 'milestoneDueChanged', 'milestoneRenamed',
      'milestoneUpdated', 'milestoneDeleted', 'ticketAdded', 'ticketRemoved', 'ticketCompleted',
      'updatePosted', 'unknown',
    ];
    for (const lng of ['en', 'ja'] as const) {
      const block = res(lng).projectActivity as Tree;
      for (const k of keys) expect(block[k], `${lng}.projectActivity.${k}`).toBeTruthy();
    }
  });

  it('健全性ラベルが3種とも定義されている', () => {
    for (const lng of ['en', 'ja'] as const) {
      const h = (res(lng).projectUpdates as Tree).health as Tree;
      expect(Object.keys(h).sort()).toEqual(['at_risk', 'off_track', 'on_track']);
    }
  });
});
