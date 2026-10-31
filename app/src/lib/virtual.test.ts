import { describe, expect, it } from 'vitest';
import { moveIndex, visibleRange } from './virtual';

// @verifies CT-EFF-005
describe('visible range', () => {
  it('covers the viewport plus the overscan rows', () => {
    const r = visibleRange({ scrollTop: 1000, viewport: 200, rowHeight: 20, count: 10_000, overscan: 3 });
    expect(r).toEqual({ start: 47, end: 63, offset: 47 * 20, total: 200_000 });
  });

  it('clamps at both ends', () => {
    expect(visibleRange({ scrollTop: 0, viewport: 100, rowHeight: 20, count: 3, overscan: 5 }).end).toBe(3);
    expect(visibleRange({ scrollTop: 0, viewport: 100, rowHeight: 20, count: 50, overscan: 5 }).start).toBe(0);
    const end = visibleRange({ scrollTop: 99_999, viewport: 100, rowHeight: 20, count: 50, overscan: 2 });
    expect(end.end).toBe(50);
    expect(end.start).toBeLessThanOrEqual(50);
  });

  it('renders no rows for an empty list', () => {
    expect(visibleRange({ scrollTop: 0, viewport: 100, rowHeight: 20, count: 0, overscan: 2 })).toEqual({
      start: 0,
      end: 0,
      offset: 0,
      total: 0,
    });
  });
});

// @verifies CT-USE-004
describe('keyboard row movement', () => {
  it('moves by one, by a page, and to the ends', () => {
    expect(moveIndex(5, 'ArrowDown', 100, 10)).toBe(6);
    expect(moveIndex(5, 'ArrowUp', 100, 10)).toBe(4);
    expect(moveIndex(5, 'PageDown', 100, 10)).toBe(15);
    expect(moveIndex(5, 'PageUp', 100, 10)).toBe(0);
    expect(moveIndex(5, 'Home', 100, 10)).toBe(0);
    expect(moveIndex(5, 'End', 100, 10)).toBe(99);
  });

  it('stays in range, and ignores other keys and empty lists', () => {
    expect(moveIndex(99, 'ArrowDown', 100, 10)).toBe(99);
    expect(moveIndex(0, 'ArrowUp', 100, 10)).toBe(0);
    expect(moveIndex(-1, 'ArrowDown', 100, 10)).toBe(0);
    expect(moveIndex(3, 'a', 100, 10)).toBe(3);
    expect(moveIndex(0, 'ArrowDown', 0, 10)).toBe(-1);
  });
});
