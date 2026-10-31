// The theme preference: light, dark, or follow the OS. This is the only first-party UI code
// that touches browser storage, and it stores the choice only, never project content.

// @implements CT-USE-005
export type ThemePref = 'system' | 'light' | 'dark';

// @implements CT-USE-005
export type Theme = 'light' | 'dark';

// @implements CT-USE-005
/** The part of `Storage` the theme needs. */
export interface PrefStore {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

// @begin CT-USE-005
export const THEME_KEY = 'clewtrace.theme';
// @end

// @implements CT-USE-005
export function parseTheme(raw: string | null): ThemePref {
  return raw === 'light' || raw === 'dark' ? raw : 'system';
}

// @implements CT-USE-005
export function resolveTheme(pref: ThemePref, osDark: boolean): Theme {
  if (pref === 'system') {
    return osDark ? 'dark' : 'light';
  }
  return pref;
}

// @implements CT-USE-005
export function loadTheme(store: PrefStore): ThemePref {
  try {
    return parseTheme(store.getItem(THEME_KEY));
  } catch {
    return 'system';
  }
}

// @implements CT-USE-005
export function saveTheme(store: PrefStore, pref: ThemePref): void {
  try {
    store.setItem(THEME_KEY, pref);
  } catch {
    // A blocked store only means the choice is not remembered.
  }
}

// @implements CT-USE-005
/** Puts the resolved theme on the root element, where the CSS tokens read it. */
export function applyTheme(root: HTMLElement, pref: ThemePref, osDark: boolean): Theme {
  const theme = resolveTheme(pref, osDark);
  root.dataset.theme = theme;
  root.style.colorScheme = theme;
  return theme;
}

// @implements CT-USE-005
/** The stored preference, from the window's local storage (UI preferences only). */
export function browserStore(): PrefStore {
  return window.localStorage;
}
