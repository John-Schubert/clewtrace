// @begin CT-EFF-005, CT-USE-007
// Tests for the paged row loader behind the problems list.
import { describe, expect, it, vi } from 'vitest';
import type { Page } from './api';
import { PagedRows } from './pages';

function fake(total: number) {
  return vi.fn(async (offset: number, limit: number): Promise<Page<number>> => {
    const rows = Array.from({ length: Math.max(0, Math.min(total - offset, limit)) }, (_, i) => offset + i);
    return { offset, total, more: offset + limit < total, rows };
  });
}

describe('PagedRows', () => {
  // @verifies CT-EFF-005
  it('learns the count from the first page and fetches later pages on demand', async () => {
    const fetch = fake(450);
    const rows = new PagedRows(fetch, () => {}, 200);
    await rows.reset();
    expect(rows.count).toBe(450);
    expect(rows.at(5)).toBe(5);
    expect(rows.at(300)).toBeUndefined();
    expect(rows.at(301)).toBeUndefined();
    await rows.idle();
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(rows.at(300)).toBe(300);
  });

  // @verifies CT-EFF-006
  it('grows the count page by page when the total is unknown, as search results do', async () => {
    const fetch = vi.fn(async (offset: number, limit: number): Promise<Page<number>> => {
      const rows = Array.from({ length: Math.max(0, Math.min(250 - offset, limit)) }, (_, i) => offset + i);
      return { offset, total: null, more: offset + limit < 250, rows };
    });
    const rows = new PagedRows(fetch, () => {}, 200);
    await rows.reset();
    // 200 rows and a placeholder for the next page, which loads when it is scrolled into view.
    expect(rows.count).toBe(201);
    expect(rows.at(200)).toBeUndefined();
    await rows.idle();
    expect(rows.count).toBe(250);
    expect(rows.at(249)).toBe(249);
  });

  // @verifies CT-USE-007
  it('has no rows and reports the error when the fetch fails', async () => {
    const onerror = vi.fn();
    const rows = new PagedRows<number>(async () => { throw new Error('boom'); }, () => {}, 200, onerror);
    await rows.reset();
    expect(rows.count).toBe(0);
    expect(onerror).toHaveBeenCalledOnce();
  });
});
// @end
