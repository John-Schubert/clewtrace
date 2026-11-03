// Palette actions for controls that live inside components: the editor bar and the trace
// panel tabs. App registers them so every button has a palette entry.
import type { ActionRegistry } from './actions';

// @implements CT-USE-002
/** What the palette needs from the open editor. */
export interface EditorHandle {
  save: () => Promise<boolean>;
  undoEdit: () => void;
  redoEdit: () => void;
}

// @implements CT-USE-002
/** Editor: save, undo, and redo. They do nothing while no editor is open. */
export function registerEditorActions(registry: ActionRegistry, editor: () => EditorHandle | undefined): void {
  registry.register({ id: 'editor.save', title: 'Editor: save the file', shortcut: 'Ctrl+S', run: async () => void (await editor()?.save()) });
  registry.register({ id: 'editor.undo', title: 'Editor: undo', shortcut: 'Ctrl+Z', run: () => editor()?.undoEdit() });
  registry.register({ id: 'editor.redo', title: 'Editor: redo', shortcut: 'Ctrl+Y', run: () => editor()?.redoEdit() });
}

// @implements CT-USE-002
export type TraceTab = 'links' | 'impact';

// @implements CT-USE-002
/** Trace: show links, and trace: show impact. */
export function registerTraceTabActions(registry: ActionRegistry, setTab: (tab: TraceTab) => void): void {
  registry.register({ id: 'trace.links', title: 'Trace: show links', run: () => setTab('links') });
  registry.register({ id: 'trace.impact', title: 'Trace: show impact', run: () => setTab('impact') });
}
