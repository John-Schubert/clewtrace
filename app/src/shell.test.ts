// @begin CT-USE-004, CT-USE-002
import { describe, expect, it } from 'vitest';
import { isPaletteShortcut, nextPane } from './shell';

describe('shell', () => {
  // @verifies CT-USE-004
  it('cycles panes forward and backward, starting from the first', () => {
    expect(nextPane(undefined, false)).toBe('browser');
    expect(nextPane('browser', false)).toBe('main');
    expect(nextPane('problems', false)).toBe('browser');
    expect(nextPane('browser', true)).toBe('problems');
  });

  // @verifies CT-USE-002
  it('recognises the palette shortcuts', () => {
    const key = (k: string, o: object = {}) => ({ key: k, ctrlKey: false, metaKey: false, shiftKey: false, ...o });
    expect(isPaletteShortcut(key('P', { ctrlKey: true, shiftKey: true }))).toBe(true);
    expect(isPaletteShortcut(key('k', { metaKey: true }))).toBe(true);
    expect(isPaletteShortcut(key('k'))).toBe(false);
    expect(isPaletteShortcut(key('p', { ctrlKey: true }))).toBe(false);
  });
});
// @end
