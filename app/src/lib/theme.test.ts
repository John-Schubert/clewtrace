import { describe, expect, it } from 'vitest';
import { applyTheme, loadTheme, parseTheme, resolveTheme, saveTheme, THEME_KEY } from './theme';

// @begin CT-USE-005
// Test support: an in-memory stand-in for the window's storage.
function fakeStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    data,
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
  };
}
// @end

// @verifies CT-USE-005
describe('theme preference', () => {
  it('accepts system, light, and dark, and falls back to system', () => {
    expect(parseTheme('light')).toBe('light');
    expect(parseTheme('dark')).toBe('dark');
    expect(parseTheme('system')).toBe('system');
    expect(parseTheme('purple')).toBe('system');
    expect(parseTheme(null)).toBe('system');
  });

  it('follows the OS setting unless overridden', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });

  it('loads the default when nothing is stored, and round-trips a choice', () => {
    const s = fakeStorage();
    expect(loadTheme(s)).toBe('system');
    saveTheme(s, 'dark');
    expect(s.data.get(THEME_KEY)).toBe('dark');
    expect(loadTheme(s)).toBe('dark');
  });

  it('survives storage that throws', () => {
    const broken = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    };
    expect(loadTheme(broken)).toBe('system');
    expect(() => saveTheme(broken, 'light')).not.toThrow();
  });

  it('sets data-theme and color-scheme on the root element', () => {
    const root = document.createElement('html');
    expect(applyTheme(root, 'system', true)).toBe('dark');
    expect(root.dataset.theme).toBe('dark');
    expect(root.style.colorScheme).toBe('dark');
    expect(applyTheme(root, 'light', true)).toBe('light');
    expect(root.dataset.theme).toBe('light');
  });
});
