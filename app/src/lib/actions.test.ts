import { describe, expect, it, vi } from 'vitest';
import { createRegistry, isPaletteKey } from './actions';

// @verifies CT-USE-002
describe('action registry', () => {
  const noop = () => {};

  it('lists registered actions in registration order', () => {
    const r = createRegistry();
    r.register({ id: 'a', title: 'Alpha', run: noop });
    r.register({ id: 'b', title: 'Beta', run: noop });
    expect(r.list().map((a) => a.id)).toEqual(['a', 'b']);
  });

  it('refuses a duplicate id', () => {
    const r = createRegistry();
    r.register({ id: 'a', title: 'Alpha', run: noop });
    expect(() => r.register({ id: 'a', title: 'Again', run: noop })).toThrow(/already/);
  });

  it('removes an action when its unregister function is called', () => {
    const r = createRegistry();
    const off = r.register({ id: 'a', title: 'Alpha', run: noop });
    off();
    expect(r.list()).toEqual([]);
  });

  it('runs an action by id and throws for an unknown id', async () => {
    const r = createRegistry();
    const run = vi.fn();
    r.register({ id: 'a', title: 'Alpha', run });
    await r.run('a');
    expect(run).toHaveBeenCalledOnce();
    await expect(r.run('nope')).rejects.toThrow(/nope/);
  });

  it('filters by every word, ignoring case, in the title or keywords', () => {
    const r = createRegistry();
    r.register({ id: 'open', title: 'Open project folder', keywords: 'recent', run: noop });
    r.register({ id: 'theme', title: 'Theme: dark', run: noop });
    expect(r.filter('OPEN fold').map((a) => a.id)).toEqual(['open']);
    expect(r.filter('recent').map((a) => a.id)).toEqual(['open']);
    expect(r.filter('zzz')).toEqual([]);
    expect(r.filter('  ').map((a) => a.id)).toEqual(['open', 'theme']);
  });

  it('ranks a title that starts with the query before one that only contains it', () => {
    const r = createRegistry();
    r.register({ id: 'x', title: 'Clear recent', run: noop });
    r.register({ id: 'y', title: 'Recent projects', run: noop });
    expect(r.filter('recent').map((a) => a.id)).toEqual(['y', 'x']);
  });
});

// @verifies CT-USE-002
describe('palette shortcut', () => {
  it('matches Ctrl+K and Cmd+K only', () => {
    expect(isPaletteKey({ key: 'k', ctrlKey: true, metaKey: false, shiftKey: false, altKey: false })).toBe(true);
    expect(isPaletteKey({ key: 'K', ctrlKey: false, metaKey: true, shiftKey: false, altKey: false })).toBe(true);
    expect(isPaletteKey({ key: 'k', ctrlKey: false, metaKey: false, shiftKey: false, altKey: false })).toBe(false);
    expect(isPaletteKey({ key: 'k', ctrlKey: true, metaKey: false, shiftKey: false, altKey: true })).toBe(false);
  });
});
