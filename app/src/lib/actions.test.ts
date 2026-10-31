// @begin CT-USE-002
// Tests for the action registry behind the command palette.
import { describe, expect, it, vi } from 'vitest';
import { ActionRegistry, filterActions } from './actions';

function sample() {
  const registry = new ActionRegistry();
  const run = vi.fn();
  registry.register({ id: 'project.open', title: 'Open project folder', run });
  registry.register({ id: 'view.theme.dark', title: 'Theme: dark', run: vi.fn() });
  registry.register({ id: 'view.theme.light', title: 'Theme: light', run: vi.fn() });
  return { registry, run };
}

describe('action registry', () => {
  // @verifies CT-USE-002
  it('lists registered actions and runs one by id', async () => {
    const { registry, run } = sample();
    expect(registry.list().map((a) => a.id)).toContain('project.open');
    await registry.run('project.open');
    expect(run).toHaveBeenCalledOnce();
  });

  // @verifies CT-USE-002
  it('rejects a duplicate id and an unknown id', async () => {
    const { registry } = sample();
    expect(() => registry.register({ id: 'project.open', title: 'x', run: () => {} })).toThrow(
      /already registered/,
    );
    await expect(registry.run('nope')).rejects.toThrow(/Unknown action/);
  });

  // @verifies CT-USE-002
  it('removes an action when its unregister function is called', () => {
    const { registry } = sample();
    const off = registry.register({ id: 'tmp', title: 'Temporary', run: () => {} });
    off();
    expect(registry.list().some((a) => a.id === 'tmp')).toBe(false);
  });

  // @verifies CT-USE-002
  it('filters by words in any order, case-insensitively, best match first', () => {
    const { registry } = sample();
    const hits = filterActions(registry.list(), 'DARK theme');
    expect(hits.map((a) => a.id)).toEqual(['view.theme.dark']);
    expect(filterActions(registry.list(), '').length).toBe(3);
    expect(filterActions(registry.list(), 'zzz')).toEqual([]);
    expect(filterActions(registry.list(), 'open')[0].id).toBe('project.open');
  });
});
// @end
