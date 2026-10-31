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
