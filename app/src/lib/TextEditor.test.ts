// @begin CT-MOD-004, CT-USE-003, CT-USE-008, CT-USE-007
// Tests for the editor component. The Rust commands are replaced by fakes.
import { flushSync, mount, unmount } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('./api', async (original) => {
  const real = (await original()) as object;
  return { ...real, sourceText: vi.fn(), saveSource: vi.fn(), editorKeywords: vi.fn() };
});
// The test web view lacks constructed style sheets in shadow roots, which a real one has.
vi.mock('./editor', async (original) => ({ ...((await original()) as object), supportsEditor: () => true }));

import type { EditorView } from '@codemirror/view';
import * as api from './api';
import TextEditor from './TextEditor.svelte';

type Instance = { view: () => EditorView | undefined; save: () => Promise<boolean>; goto: (line: number) => void };
let instance: Instance | undefined;
afterEach(() => {
  if (instance) unmount(instance as unknown as Record<string, unknown>);
  instance = undefined;
  document.body.innerHTML = '';
  vi.clearAllMocks();
});

const settle = async () => {
  for (let i = 0; i < 5; i++) {
    await Promise.resolve();
    await new Promise((r) => setTimeout(r, 0));
    flushSync();
  }
};

async function open(props: { path?: string; line?: number; ondirty?: (d: boolean) => void } = {}) {
  vi.mocked(api.sourceText).mockResolvedValue({ path: 'a.sysml', text: 'package A {\n  part x;\n  part y;\n}\n' });
  vi.mocked(api.editorKeywords).mockResolvedValue(['package', 'part']);
  instance = mount(TextEditor, {
    target: document.body,
    props: { path: 'a.sysml', line: 1, ondirty: () => {}, ...props },
  }) as unknown as Instance;
  await settle();
  return instance;
}

const button = (name: string) =>
  [...document.querySelectorAll('button')].find((b) => b.textContent?.trim() === name) as HTMLButtonElement;
const type = (view: EditorView, text: string) =>
  view.dispatch({ changes: { from: view.state.doc.length, insert: text }, userEvent: 'input.type' });

describe('TextEditor', () => {
  // @verifies CT-MOD-004
  it('opens the whole file and puts the cursor on the line asked for', async () => {
    const editor = await open({ line: 3 });
    const view = editor.view()!;
    expect(view.state.doc.toString()).toContain('part y;');
    expect(view.state.doc.lineAt(view.state.selection.main.head).number).toBe(3);
    expect(api.sourceText).toHaveBeenCalledWith('a.sysml');
    expect(document.querySelector('h2')?.textContent).toContain('a.sysml');
  });

  // @verifies CT-MOD-004
  it('moves to another line of the same file without reloading it', async () => {
    const editor = await open({ line: 1 });
    const view = editor.view()!;
    // happy-dom re-enters the view when a focused view moves the selection; a real web view
    // does not. Taking the focus away keeps the test on the editor's own logic.
    view.contentDOM.blur();
    vi.spyOn(view, 'focus').mockImplementation(() => {});
    editor.goto(3);
    flushSync();
    expect(view.state.doc.lineAt(view.state.selection.main.head).number).toBe(3);
    expect(api.sourceText).toHaveBeenCalledTimes(1);
  });

  // @verifies CT-USE-007
  it('says what went wrong when the file cannot be opened', async () => {
    vi.mocked(api.sourceText).mockRejectedValue(new Error('file not found'));
    vi.mocked(api.editorKeywords).mockResolvedValue([]);
    instance = mount(TextEditor, { target: document.body, props: { path: 'gone.sysml', line: 1, ondirty: () => {} } }) as unknown as Instance;
    await settle();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('file not found');
  });

  // @verifies CT-USE-008
  it('saves only when asked, with the whole text, then shows it is saved', async () => {
    const ondirty = vi.fn();
    const editor = await open({ ondirty });
    vi.mocked(api.saveSource).mockResolvedValue({ path: 'a.sysml', bytes: 1 });
    expect(button('Save').disabled).toBe(true);
    type(editor.view()!, '// edited\n');
    flushSync();
    expect(ondirty).toHaveBeenLastCalledWith(true);
    expect(document.body.textContent).toContain('Unsaved changes');
    expect(api.saveSource).not.toHaveBeenCalled();
    button('Save').click();
    await settle();
    expect(api.saveSource).toHaveBeenCalledWith('a.sysml', 'package A {\n  part x;\n  part y;\n}\n// edited\n');
    expect(ondirty).toHaveBeenLastCalledWith(false);
    expect(document.body.textContent).toContain('Saved');
    expect(button('Save').disabled).toBe(true);
  });

  // @verifies CT-USE-008, CT-USE-007
  it('keeps the edits and says why when a save fails', async () => {
    const editor = await open();
    vi.mocked(api.saveSource).mockRejectedValue(new Error('the file could not be saved: read-only'));
    type(editor.view()!, 'x');
    flushSync();
    button('Save').click();
    await settle();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('read-only');
    expect(button('Save').disabled).toBe(false);
    expect(editor.view()!.state.doc.toString().endsWith('x')).toBe(true);
  });

  // @verifies CT-USE-003
  it('undoes and redoes from its buttons', async () => {
    const editor = await open();
    const original = editor.view()!.state.doc.toString();
    type(editor.view()!, '// more\n');
    flushSync();
    button('Undo').click();
    flushSync();
    expect(editor.view()!.state.doc.toString()).toBe(original);
    button('Redo').click();
    flushSync();
    expect(editor.view()!.state.doc.toString()).toBe(original + '// more\n');
  });

  // @verifies CT-USE-008
  it('is not dirty again after undoing back to the saved text', async () => {
    const ondirty = vi.fn();
    const editor = await open({ ondirty });
    type(editor.view()!, 'x');
    flushSync();
    button('Undo').click();
    flushSync();
    expect(ondirty).toHaveBeenLastCalledWith(false);
  });
});
// @end
