// @begin CT-USE-005
// Tests for the theme choice: OS setting by default, manual override, and the stored preference.
import { beforeEach, describe, expect, it } from 'vitest';
import { applyTheme, loadPreference, resolveTheme, savePreference, THEME_KEY } from './theme';

describe('theme', () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
  });

  // @verifies CT-USE-005
  it('follows the OS setting by default', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
  });

  // @verifies CT-USE-005
  it('lets a manual choice override the OS setting', () => {
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });

  // @verifies CT-USE-005
  it('sets the theme on the root element', () => {
    applyTheme(document.documentElement, 'dark');
    expect(document.documentElement.dataset.theme).toBe('dark');
  });

  // @verifies CT-USE-005
  it('stores only the preference and ignores unknown stored values', () => {
    expect(loadPreference()).toBe('system');
    savePreference('dark');
    expect(localStorage.getItem(THEME_KEY)).toBe('dark');
    expect(loadPreference()).toBe('dark');
    localStorage.setItem(THEME_KEY, 'purple');
    expect(loadPreference()).toBe('system');
  });
});
// @end
