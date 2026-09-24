/**
 * useFocusTrap.test.ts — フォーカストラップのテスト
 *
 * 注: vitest.config の environment が 'node' のため、DOM テスト（Esc・復帰）は実装していません。
 * 純粋関数 getNextFocusIndex の循環動作のみをテストします。
 */

import { describe, it, expect } from 'vitest';
import { getNextFocusIndex } from './useFocusTrap';

describe('useFocusTrap', () => {
  describe('getNextFocusIndex', () => {
    describe('Tab (forward navigation)', () => {
      it('should go to index 0 when current is -1', () => {
        expect(getNextFocusIndex(-1, 3, false)).toBe(0);
      });

      it('should go to next index when in the middle', () => {
        expect(getNextFocusIndex(0, 3, false)).toBe(1);
        expect(getNextFocusIndex(1, 3, false)).toBe(2);
      });

      it('should wrap to 0 when at the end', () => {
        expect(getNextFocusIndex(2, 3, false)).toBe(0);
      });

      it('should handle single element', () => {
        expect(getNextFocusIndex(0, 1, false)).toBe(0);
      });

      it('should handle 0 elements', () => {
        expect(getNextFocusIndex(-1, 0, false)).toBe(-1);
        expect(getNextFocusIndex(0, 0, false)).toBe(-1);
      });

      it('should handle out-of-range current index', () => {
        expect(getNextFocusIndex(10, 3, false)).toBe(0);
      });
    });

    describe('Shift+Tab (backward navigation)', () => {
      it('should go to last index when current is -1', () => {
        expect(getNextFocusIndex(-1, 3, true)).toBe(2);
      });

      it('should go to previous index when in the middle', () => {
        expect(getNextFocusIndex(2, 3, true)).toBe(1);
        expect(getNextFocusIndex(1, 3, true)).toBe(0);
      });

      it('should wrap to last index when at the start', () => {
        expect(getNextFocusIndex(0, 3, true)).toBe(2);
      });

      it('should handle single element', () => {
        expect(getNextFocusIndex(0, 1, true)).toBe(0);
      });

      it('should handle 0 elements', () => {
        expect(getNextFocusIndex(-1, 0, true)).toBe(-1);
        expect(getNextFocusIndex(0, 0, true)).toBe(-1);
      });

      it('should handle out-of-range current index', () => {
        expect(getNextFocusIndex(10, 3, true)).toBe(2);
      });
    });

    describe('cycle behavior', () => {
      it('should cycle forward through multiple elements', () => {
        const count = 4;
        let current = -1;
        for (let i = 0; i < count + 2; i++) {
          current = getNextFocusIndex(current, count, false);
          expect(current).toBeGreaterThanOrEqual(0);
          expect(current).toBeLessThan(count);
        }
      });

      it('should cycle backward through multiple elements', () => {
        const count = 4;
        let current = -1;
        for (let i = 0; i < count + 2; i++) {
          current = getNextFocusIndex(current, count, true);
          expect(current).toBeGreaterThanOrEqual(0);
          expect(current).toBeLessThan(count);
        }
      });
    });
  });
});
