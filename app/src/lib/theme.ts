// Light and dark themes: the choice, the stored preference, and applying it to the page.
// The preference is the only thing the UI keeps in browser storage (CT-SEC-003).

// @implements CT-USE-005
/** What the user picked. `system` follows the OS setting. */
export type ThemePreference = 'system' | 'light' | 'dark';

// @implements CT-USE-005
export type Theme = 'light' | 'dark';

// @implements CT-USE-005
export const THEME_KEY = 'clewtrace.theme';

// @implements CT-USE-005
/** The theme to show for a preference, given whether the OS asks for dark. */
export function resolveTheme(preference: ThemePreference, systemDark: boolean): Theme {
  if (preference === 'system') {
    return systemDark ? 'dark' : 'light';
  }
  return preference;
}

// @implements CT-USE-005
/** Sets `data-theme` on the root element; the CSS tokens in tokens.css key off it. */
export function applyTheme(root: HTMLElement, theme: Theme): void {
  root.dataset.theme = theme;
}

// @implements CT-USE-005
/** The stored preference, or `system` when none is stored or the value is unknown. */
export function loadPreference(): ThemePreference {
  const stored = localStorage.getItem(THEME_KEY);
  return stored === 'light' || stored === 'dark' ? stored : 'system';
}

// @implements CT-USE-005
export function savePreference(preference: ThemePreference): void {
  if (preference === 'system') {
    localStorage.removeItem(THEME_KEY);
  } else {
    localStorage.setItem(THEME_KEY, preference);
  }
}
