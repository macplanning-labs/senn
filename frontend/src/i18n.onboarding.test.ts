/**
 * i18n.onboarding.test.ts - i18n キー整合性テスト
 *
 * onboarding.* キーが EN/JA 両方に存在し、
 * 空でないことを検証する
 */

import { describe, it, expect } from 'vitest';
import i18n from './i18n';

describe('i18n onboarding keys consistency', () => {
  it('should have onboarding keys in both EN and JA', () => {
    const enResources = (i18n.options.resources?.en?.translation) as Record<string, Record<string, any>>;
    const jaResources = (i18n.options.resources?.ja?.translation) as Record<string, Record<string, any>>;

    const onboardingKeys = [
      'checklistTitle',
      'stepTeam',
      'stepTeamHint',
      'stepChoose',
      'stepChooseLocked',
      'optionTicketTitle',
      'optionTicketHint',
      'createTicket',
      'optionProjectTitle',
      'optionProjectHint',
      'optionDemoTitle',
      'stepDone',
      'createProject',
      'teamProjectsCreateFirst',
      'teamProjectsHint',
      'openGantt',
      'openDependencies',
      'openTickets',
    ];

    onboardingKeys.forEach((key) => {
      // EN キーが存在し、空でないことを確認
      const enKey = enResources?.onboarding?.[key];
      expect(enKey).toBeTruthy();
      expect(typeof enKey).toBe('string');

      // JA キーが存在し、空でないことを確認
      const jaKey = jaResources?.onboarding?.[key];
      expect(jaKey).toBeTruthy();
      expect(typeof jaKey).toBe('string');
    });
  });

  it('should not have myIssues.noTeamGuide (removed in S2-6)', () => {
    const enResources = (i18n.options.resources?.en?.translation) as Record<string, Record<string, any>>;
    const jaResources = (i18n.options.resources?.ja?.translation) as Record<string, Record<string, any>>;

    // S2-6 で削除されたことを確認
    expect(enResources?.myIssues?.noTeamGuide).toBeUndefined();
    expect(jaResources?.myIssues?.noTeamGuide).toBeUndefined();
  });
});
