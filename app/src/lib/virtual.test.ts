// @begin CT-EFF-005, CT-EFF-004
// Tests for the window arithmetic of the shared virtual list.
import { describe, expect, it } from 'vitest';
import { scrollTopFor, visibleRange } from './virtual';

describe('virtual list window', () => {
  // @verifies CT-EFF-005
  it('renders only the rows in view plus an overscan, however long the list is', () => {
    const r = visibleRange({ count: 1_000_000, rowHeight: 20, viewport: 400, scrollTop: 0, overscan: 5 });
    expect(r.start).toBe(0);
    expect(r.end).toBe(25);
    expect(r.totalHeight).toBe(20_000_000);
  });

  // @verifies CT-EFF-005
  it('moves the window with the scroll position', () => {
    const r = visibleRange({ count: 10_000, rowHeight: 20, viewport: 400, scrollTop: 2000, overscan: 5 });
    expect(r.start).toBe(95);
    expect(r.end).toBe(125);
    expect(r.offset).toBe(95 * 20);
  });

  // @verifies CT-EFF-005
  it('clamps at the end and handles an empty list', () => {
    const end = visibleRange({ count: 30, rowHeight: 20, viewport: 400, scrollTop: 10_000, overscan: 5 });
    expect(end.end).toBe(30);
    expect(end.start).toBeLessThanOrEqual(30);
    const none = visibleRange({ count: 0, rowHeight: 20, viewport: 400, scrollTop: 0, overscan: 5 });
    expect(none).toEqual({ start: 0, end: 0, offset: 0, totalHeight: 0 });
  });

  // @verifies CT-EFF-005
  it('scrolls the least needed to bring a row into view', () => {
    expect(scrollTopFor({ index: 50, rowHeight: 20, viewport: 400, scrollTop: 0 })).toBe(620);
    expect(scrollTopFor({ index: 2, rowHeight: 20, viewport: 400, scrollTop: 200 })).toBe(40);
    expect(scrollTopFor({ index: 12, rowHeight: 20, viewport: 400, scrollTop: 200 })).toBe(200);
  });
});
// @end
