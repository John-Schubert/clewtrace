// @begin CT-MOD-005, CT-EFF-005
// Tests for the lazy model tree: it fetches only the pages the user reaches.
import { describe, expect, it, vi } from 'vitest';
import type { Page, TreeRow } from './api';
import { TreeModel } from './tree';

// A tree where element 0 is the root. `kids` maps a parent to its child count; child ids
// are parent * 1000 + offset + 1, so a test can read the id of any row.
function fake(kids: Record<number, number>) {
  const fetch = vi.fn(async (parent: number, offset: number, limit: number): Promise<Page<TreeRow>> => {
    const total = kids[parent] ?? 0;
    const rows: TreeRow[] = [];
    for (let i = offset; i < Math.min(total, offset + limit); i++) {
      const id = parent * 1000 + i + 1;
      rows.push({ id, kind: 'package', name: `n${id}`, shortName: null, children: kids[id] ?? 0, file: 'a.sysml', line: 1 });
    }
    return { offset, total, more: offset + limit < total, rows };
  });
  return fetch;
}

describe('TreeModel', () => {
  // @verifies CT-MOD-005
  it('loads the first page of the root on reset', async () => {
    const fetch = fake({ 0: 3 });
    const tree = new TreeModel(fetch, () => {});
    await tree.reset();
    expect(tree.count).toBe(3);
    expect(tree.at(0)?.row?.id).toBe(1);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  // @verifies CT-MOD-005, CT-EFF-005
  it('expands a node by inserting its children after it, fetching one page only', async () => {
    const fetch = fake({ 0: 2, 1: 450 });
    const tree = new TreeModel(fetch, () => {}, 200);
    await tree.reset();
    await tree.expand(0);
    expect(tree.count).toBe(2 + 450);
    expect(fetch).not.toHaveBeenCalledWith(1, 200, 200);
    expect(tree.at(0)?.expanded).toBe(true);
    expect(tree.at(1)?.row?.id).toBe(1001);
    expect(tree.at(1)?.depth).toBe(1);
    expect(tree.at(450)?.row).toBeUndefined();
    expect(tree.at(451)?.row?.id).toBe(2);
    expect(fetch).toHaveBeenCalledWith(1, 0, 200);
  });

  // @verifies CT-EFF-005
  it('loads a later page only when its row is asked for', async () => {
    const fetch = fake({ 0: 1, 1: 450 });
    const changed = vi.fn();
    const tree = new TreeModel(fetch, changed, 200);
    await tree.reset();
    await tree.expand(0);
    fetch.mockClear();
    expect(tree.at(300)?.row).toBeUndefined();
    expect(tree.at(300)?.row).toBeUndefined();
    await tree.idle();
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledWith(1, 200, 200);
    expect(tree.at(300)?.row?.id).toBe(1300);
    expect(changed).toHaveBeenCalled();
  });

  // @verifies CT-MOD-005
  it('collapses a node and everything below it', async () => {
    const fetch = fake({ 0: 2, 1: 2, 1001: 3 });
    const tree = new TreeModel(fetch, () => {});
    await tree.reset();
    await tree.expand(0);
    await tree.expand(1);
    expect(tree.count).toBe(2 + 2 + 3);
    tree.collapse(0);
    expect(tree.count).toBe(2);
    expect(tree.at(0)?.expanded).toBe(false);
    expect(tree.at(1)?.row?.id).toBe(2);
  });

  // @verifies CT-MOD-005
  it('ignores a page that arrives after its parent was collapsed', async () => {
    const fetch = fake({ 0: 1, 1: 450 });
    const tree = new TreeModel(fetch, () => {}, 200);
    await tree.reset();
    await tree.expand(0);
    tree.at(300);
    tree.collapse(0);
    await tree.idle();
    expect(tree.count).toBe(1);
  });

  // @verifies CT-MOD-005
  it('does nothing when a leaf is expanded and finds a row by element id', async () => {
    const tree = new TreeModel(fake({ 0: 2, 1: 1 }), () => {});
    await tree.reset();
    await tree.expand(1);
    expect(tree.count).toBe(2);
    await tree.expand(0);
    expect(tree.indexOf(1001)).toBe(1);
    expect(tree.indexOf(99)).toBe(-1);
  });

  // @verifies CT-USE-007
  it('reports a failed fetch instead of throwing', async () => {
    const onerror = vi.fn();
    const tree = new TreeModel(async () => { throw new Error('boom'); }, () => {}, 200, onerror);
    await tree.reset();
    expect(onerror).toHaveBeenCalledOnce();
    expect(tree.count).toBe(0);
  });
  // @verifies CT-MOD-005
  it('refreshes in place: expanded packages stay open and new children appear', async () => {
    const kids: Record<number, number> = { 0: 2, 1: 3 };
    const fetch = fake(kids);
    const counts: number[] = [];
    const watching = new TreeModel(fetch, () => counts.push(watching.count), 200);
    await watching.reset();
    await watching.expand(0);
    expect(watching.count).toBe(2 + 3);
    kids[1] = 4;
    kids[0] = 3;
    counts.length = 0;
    await watching.refresh();
    // The tree is swapped in one step, so it is never shown empty or half built.
    expect(counts.every((n) => n === 3 + 4)).toBe(true);
    expect(watching.count).toBe(3 + 4);
    expect(watching.at(0)?.expanded).toBe(true);
    expect(watching.at(4)?.row?.id).toBe(1004);
  });

  // @verifies CT-MOD-005
  it('refreshes a package that is gone as closed, and keeps the ids of the rest', async () => {
    const kids: Record<number, number> = { 0: 2, 1: 3, 2: 1 };
    const tree = new TreeModel(fake(kids), () => {}, 200);
    await tree.reset();
    await tree.expand(0);
    await tree.expand(tree.indexOf(2));
    expect(tree.count).toBe(2 + 3 + 1);
    kids[0] = 1;
    delete kids[2];
    await tree.refresh();
    expect(tree.count).toBe(1 + 3);
    expect(tree.indexOf(1)).toBe(0);
    expect(tree.indexOf(2)).toBe(-1);
  });

  // @verifies CT-MOD-005, CT-USE-007
  it('keeps the old tree when a refresh fails', async () => {
    let fail = false;
    const base = fake({ 0: 2 });
    const onerror = vi.fn();
    const tree = new TreeModel(async (...args) => (fail ? Promise.reject(new Error('boom')) : base(...args)), () => {}, 200, onerror);
    await tree.reset();
    fail = true;
    await tree.refresh();
    expect(tree.count).toBe(2);
    expect(onerror).toHaveBeenCalledOnce();
  });

  // @verifies CT-MOD-005
  it('ignores a refresh that a reset overtook', async () => {
    const tree = new TreeModel(fake({ 0: 2 }), () => {}, 200);
    await tree.reset();
    const late = tree.refresh();
    await tree.reset();
    await late;
    expect(tree.count).toBe(2);
  });
});
// @end
