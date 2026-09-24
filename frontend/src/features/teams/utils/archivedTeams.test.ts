/**
 * archivedTeams.test.ts — チームアーカイブユーティリティのテスト
 */

import { describe, it, expect } from 'vitest';
import { isArchived, activeTeams, archivedTeams } from './archivedTeams';

describe('archivedTeams utilities', () => {
  const mockTeams = [
    { id: 1, name: 'Active Team 1', archivedAt: null },
    { id: 2, name: 'Active Team 2', archivedAt: undefined },
    { id: 3, name: 'Archived Team 1', archivedAt: '2026-09-19T00:00:00Z' },
    { id: 4, name: 'Archived Team 2', archivedAt: '2026-09-18T00:00:00Z' },
  ];

  describe('isArchived', () => {
    it('should return false for team with archivedAt: null', () => {
      expect(isArchived({ archivedAt: null })).toBe(false);
    });

    it('should return false for team with archivedAt: undefined', () => {
      expect(isArchived({ archivedAt: undefined })).toBe(false);
    });

    it('should return false for team with missing archivedAt', () => {
      expect(isArchived({})).toBe(false);
    });

    it('should return true for team with archivedAt timestamp', () => {
      expect(isArchived({ archivedAt: '2026-09-19T00:00:00Z' })).toBe(true);
    });
  });

  describe('activeTeams', () => {
    it('should filter out archived teams', () => {
      const result = activeTeams(mockTeams);
      expect(result).toHaveLength(2);
      expect(result.map((t) => t.id)).toEqual([1, 2]);
    });

    it('should return empty array for null input', () => {
      expect(activeTeams(null)).toEqual([]);
    });

    it('should return empty array for undefined input', () => {
      expect(activeTeams(undefined)).toEqual([]);
    });

    it('should return empty array for empty array input', () => {
      expect(activeTeams([])).toEqual([]);
    });

    it('should handle mixed null and undefined archivedAt', () => {
      const teams = [
        { id: 1, archivedAt: null },
        { id: 2, archivedAt: undefined },
      ];
      expect(activeTeams(teams)).toHaveLength(2);
    });
  });

  describe('archivedTeams', () => {
    it('should filter to only archived teams', () => {
      const result = archivedTeams(mockTeams);
      expect(result).toHaveLength(2);
      expect(result.map((t) => t.id)).toEqual([3, 4]);
    });

    it('should return empty array for null input', () => {
      expect(archivedTeams(null)).toEqual([]);
    });

    it('should return empty array for undefined input', () => {
      expect(archivedTeams(undefined)).toEqual([]);
    });

    it('should return empty array for empty array input', () => {
      expect(archivedTeams([])).toEqual([]);
    });

    it('should return empty array when no teams are archived', () => {
      const teams = [
        { id: 1, archivedAt: null },
        { id: 2, archivedAt: undefined },
      ];
      expect(archivedTeams(teams)).toEqual([]);
    });
  });
});
