// Shell logic kept out of the component: the panes, the focus cycle, and the palette shortcut.

// @implements CT-USE-004
export const PANES = ['browser', 'main', 'trace', 'problems'] as const;

// @implements CT-USE-004
export type Pane = (typeof PANES)[number];

// @implements CT-USE-004
/** The next pane in the focus order (F6), or the previous one with `backwards`. */
export function nextPane(current: Pane | undefined, backwards: boolean): Pane {
  const at = current ? PANES.indexOf(current) : -1;
  const step = backwards ? -1 : 1;
  return PANES[(at + step + PANES.length) % PANES.length];
}

// @implements CT-USE-002
/** True for Ctrl+Shift+P or Cmd+Shift+P, and for Ctrl+K or Cmd+K. */
export function isPaletteShortcut(event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey'>): boolean {
  const key = event.key.toLowerCase();
  const mod = event.ctrlKey || event.metaKey;
  return mod && ((event.shiftKey && key === 'p') || (!event.shiftKey && key === 'k'));
}
