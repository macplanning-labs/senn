/**
 * i18n.team-modal.test.ts - i18n キー整合性テスト
 *
 * team.modal.* と myIssues.noTeamGuide が EN/JA 両方に存在し、
 * 空でないことを検証する
 */

import { describe, it, expect } from 'vitest';
import i18n from './i18n';

describe('i18n key consistency', () => {
  it('should have team keys in both EN and JA', () => {
    const enResources = (i18n.options.resources?.en?.translation) as Record<string, Record<string, any>>;
    const jaResources = (i18n.options.resources?.ja?.translation) as Record<string, Record<string, any>>;

    const teamKeys = [
      'general',
      'dangerZone',
      'delete',
      'deleteTeamWarning',
      'deleteConfirm',
    ];

    teamKeys.forEach((key) => {
      // EN キーが存在し、空でないことを確認
      const enKey = enResources?.team?.[key];
      expect(enKey).toBeTruthy();
      expect(typeof enKey).toBe('string');

      // JA キーが存在し、空でないことを確認
      const jaKey = jaResources?.team?.[key];
      expect(jaKey).toBeTruthy();
      expect(typeof jaKey).toBe('string');
    });
  });

  it('should have team.modal keys in both EN and JA', () => {
    const enResources = (i18n.options.resources?.en?.translation) as Record<string, Record<string, any>>;
    const jaResources = (i18n.options.resources?.ja?.translation) as Record<string, Record<string, any>>;

    const modalKeys = [
      'titleCreate',
      'titleEdit',
      'icon',
      'color',
      'name',
      'namePlaceholder',
      'prefix',
      'prefixPlaceholder',
      'prefixHelp',
      'description',
      'webhook',
      'active',
      'cancel',
      'submitCreate',
      'submitUpdate',
      'saving',
      'nameRequired',
      'prefixRequired',
      'prefixInvalid',
      'created',
      'updated',
      'deleted',
      'createFailed',
      'updateFailed',
      'deleteFailed',
    ];

    modalKeys.forEach((key) => {
      // EN キーが存在し、空でないことを確認
      const enKey = enResources?.team?.modal?.[key];
      expect(enKey).toBeTruthy();
      expect(typeof enKey).toBe('string');

      // JA キーが存在し、空でないことを確認
      const jaKey = jaResources?.team?.modal?.[key];
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

  it('should not have orphaned keys sidebar.manageTeams and myIssues.goToTeams', () => {
    const enResources = (i18n.options.resources?.en?.translation) as Record<string, Record<string, any>>;
    const jaResources = (i18n.options.resources?.ja?.translation) as Record<string, Record<string, any>>;

    // これらのキーが削除されていることを確認
    expect(enResources?.sidebar?.manageTeams).toBeUndefined();
    expect(jaResources?.sidebar?.manageTeams).toBeUndefined();
    expect(enResources?.myIssues?.goToTeams).toBeUndefined();
    expect(jaResources?.myIssues?.goToTeams).toBeUndefined();
  });
});
