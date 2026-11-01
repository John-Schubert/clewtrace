// @begin CT-MOD-004, CT-USE-003, CT-USE-008, CT-USE-007
// Tests for the editor component. The Rust commands are replaced by fakes.
import { flushSync, mount, unmount } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('./api', async (original) => {
  const real = (await original()) as object;
  return { ...real, sourceText: vi.fn(), saveSource: vi.fn(), editorKeywords: vi.fn(), validateSource: vi.fn() };
});
// The test web view lacks constructed style sheets in shadow roots, which a real one has.
vi.mock('./editor', async (original) => ({ ...((await original()) as object), supportsEditor: () => true }));

import { forEachDiagnostic } from '@codemirror/lint';
import type { EditorView } from '@codemirror/view';
import * as api from './api';
import TextEditor from './TextEditor.svelte';

type Instance = {
  view: () => EditorView | undefined;
  save: () => Promise<boolean>;
  goto: (line: number) => void;
  reloadFromDisk: () => Promise<void>;
  hold: () => void;
  release: (keep: boolean) => void;
};
let instance: Instance | undefined;
afterEach(async () => {
  if (instance) unmount(instance as unknown as Record<string, unknown>);
  instance = undefined;
  document.body.innerHTML = '';
  vi.clearAllMocks();
  // A focus change makes CodeMirror check focus again on a 10 ms timer, which outlives
  // the editor. If it fires after the next test focuses its own editor, happy-dom throws
  // while it looks for the focused element across the two shadow roots. Waiting longer
  // than the timer lets it fire before the next test starts.
  await new Promise((resolve) => setTimeout(resolve, 20));
});

const settle = async () => {
  for (let i = 0; i < 5; i++) {
    await Promise.resolve();
    await new Promise((r) => setTimeout(r, 0));
    flushSync();
  }
};

const reply = (diagnostics: api.DiagnosticRow[] = [], revision = 1): api.EditReply => ({
  revision,
  diagnostics,
  summary: {} as api.ProjectSummary,
});
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
/** The markers the editor shows, as `from-to:severity:message`. */
const shown = (view: EditorView) => {
  const found: string[] = [];
  forEachDiagnostic(view.state, (d, from, to) => found.push(`${from}-${to}:${d.severity}:${d.message}`));
  return found;
};

async function open(
  props: {
    path?: string;
    line?: number;
    ondirty?: (d: boolean) => void;
    quietMs?: number;
    autosaveMs?: number;
    onvalidated?: (r: api.EditReply) => void;
    validated?: api.EditReply;
  } = {},
) {
  const { validated, ...componentProps } = props;
  vi.mocked(api.validateSource).mockResolvedValue(validated ?? reply());
  vi.mocked(api.sourceText).mockResolvedValue({ path: 'a.sysml', text: 'package A {\n  part x;\n  part y;\n}\n' });
  vi.mocked(api.editorKeywords).mockResolvedValue(['package', 'part']);
  instance = mount(TextEditor, {
    target: document.body,
    // Autosave waits a minute unless a test asks for less, so the other tests see no save.
    props: { path: 'a.sysml', line: 1, ondirty: () => {}, onvalidated: () => {}, quietMs: 20, autosaveMs: 60_000, ...componentProps },
  }) as unknown as Instance;
  await settle();
  return instance;
}

const button = (name: string) =>
  [...document.querySelectorAll('button')].find((b) => b.textContent?.trim() === name) as HTMLButtonElement;
const type = (view: EditorView, text: string) =>
  view.dispatch({ changes: { from: view.state.doc.length, insert: text }, userEvent: 'input.type' });

// @end

// @verifies CT-MOD-004, CT-MOD-003, CT-USE-003, CT-USE-008, CT-USE-007
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
    instance = mount(TextEditor, { target: document.body, props: { path: 'gone.sysml', line: 1, ondirty: () => {}, onvalidated: () => {} } }) as unknown as Instance;
    await settle();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('file not found');
  });

  // @verifies CT-USE-008
  it('saves on Save before the pause ends, with the whole text, then shows it is saved', async () => {
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

  // @verifies CT-USE-008
  it('saves by itself after a short pause in typing', async () => {
    const ondirty = vi.fn();
    const editor = await open({ ondirty, autosaveMs: 30 });
    vi.mocked(api.saveSource).mockResolvedValue({ path: 'a.sysml', bytes: 1 });
    type(editor.view()!, 'a');
    type(editor.view()!, 'b');
    flushSync();
    expect(api.saveSource).not.toHaveBeenCalled();
    await wait(60);
    await settle();
    expect(api.saveSource).toHaveBeenCalledTimes(1);
    expect(vi.mocked(api.saveSource).mock.calls[0][1].endsWith('ab')).toBe(true);
    expect(ondirty).toHaveBeenLastCalledWith(false);
    expect(document.body.textContent).toContain('Saved');
  });

  // @verifies CT-USE-008
  it('saves when the editor loses focus', async () => {
    const editor = await open();
    vi.mocked(api.saveSource).mockResolvedValue({ path: 'a.sysml', bytes: 1 });
    const view = editor.view()!;
    view.focus();
    type(view, 'z');
    flushSync();
    view.contentDOM.dispatchEvent(new FocusEvent('blur'));
    await settle();
    expect(api.saveSource).toHaveBeenCalledTimes(1);
    expect(vi.mocked(api.saveSource).mock.calls[0][1].endsWith('z')).toBe(true);
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

// @begin CT-MOD-003, CT-MOD-004, CT-USE-007, CT-EFF-008
describe('TextEditor live validation', () => {
  const broken = (over: Partial<api.DiagnosticRow> = {}): api.DiagnosticRow => ({
    severity: 'error',
    what: 'cannot find `nope`',
    where: 'a.sysml:2:8',
    help: 'Declare it or fix the name.',
    line: 2,
    column: 8,
    ...over,
  });

  // @verifies CT-MOD-003, CT-EFF-008
  it('sends the text once after a pause in typing, not on each keystroke', async () => {
    const onvalidated = vi.fn();
    const editor = await open({ onvalidated });
    // Opening checks the saved text once, for its markers.
    expect(api.validateSource).toHaveBeenCalledTimes(1);
    for (const ch of ['a', 'b', 'c']) {
      type(editor.view()!, ch);
      await wait(5);
    }
    expect(api.validateSource).toHaveBeenCalledTimes(1);
    await wait(80);
    expect(api.validateSource).toHaveBeenCalledTimes(2);
    expect(api.validateSource).toHaveBeenLastCalledWith('a.sysml', 'package A {\n  part x;\n  part y;\n}\nabc');
    expect(onvalidated).toHaveBeenCalledTimes(1);
  });

  // @verifies CT-MOD-003, CT-USE-007
  it('shows the problems of the file as markers with their help text', async () => {
    const editor = await open({ validated: reply([broken()], 2) });
    await settle();
    const [marker] = shown(editor.view()!);
    expect(marker).toContain('error:cannot find `nope`');
    expect(marker).toContain('Declare it or fix the name.');
    vi.mocked(api.validateSource).mockResolvedValue(reply([], 3));
    type(editor.view()!, '\n');
    await wait(80);
    expect(shown(editor.view()!)).toEqual([]);
  });

  // @verifies CT-MOD-003
  it('shows the markers of the saved text on open without refreshing the other panes', async () => {
    const onvalidated = vi.fn();
    const editor = await open({ onvalidated, validated: reply([broken()]) });
    await settle();
    expect(shown(editor.view()!)).toHaveLength(1);
    expect(onvalidated).not.toHaveBeenCalled();
  });

  // @verifies CT-MOD-003
  it('drops the markers of a reply that further typing overtook', async () => {
    const editor = await open();
    let release!: (r: api.EditReply) => void;
    vi.mocked(api.validateSource).mockReturnValueOnce(new Promise((r) => (release = r)));
    type(editor.view()!, 'a');
    await wait(60);
    type(editor.view()!, 'b');
    release(reply([broken()]));
    await settle();
    expect(shown(editor.view()!)).toEqual([]);
  });

  // @verifies CT-MOD-003, CT-EFF-008
  it('keeps one check in flight, then checks the newest text', async () => {
    const editor = await open();
    let release!: (r: api.EditReply) => void;
    vi.mocked(api.validateSource).mockReturnValueOnce(new Promise((r) => (release = r)));
    type(editor.view()!, 'a');
    await wait(60);
    type(editor.view()!, 'b');
    await wait(60);
    // The second pause ended while the first check was out, so nothing more was sent.
    expect(api.validateSource).toHaveBeenCalledTimes(2);
    release(reply());
    await wait(80);
    expect(api.validateSource).toHaveBeenCalledTimes(3);
    expect(api.validateSource).toHaveBeenLastCalledWith('a.sysml', 'package A {\n  part x;\n  part y;\n}\nab');
  });

  // @verifies CT-MOD-003, CT-USE-007
  it('says why a check failed and keeps the editor usable', async () => {
    const editor = await open();
    vi.mocked(api.validateSource).mockRejectedValue(new Error('trace.toml:9: top_level_definitions no longer resolves'));
    type(editor.view()!, 'x');
    await wait(80);
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('top_level_definitions');
    type(editor.view()!, 'y');
    expect(editor.view()!.state.doc.toString().endsWith('xy')).toBe(true);
    vi.mocked(api.validateSource).mockResolvedValue(reply());
    await wait(80);
    expect(document.querySelector('[role="alert"]')).toBeNull();
  });

  // @verifies CT-MOD-003, CT-USE-008
  it('checks the text at once when it is saved, without waiting for the pause', async () => {
    const editor = await open({ quietMs: 5000 });
    vi.mocked(api.saveSource).mockResolvedValue({ path: 'a.sysml', bytes: 1 });
    type(editor.view()!, 'z');
    flushSync();
    expect(api.validateSource).toHaveBeenCalledTimes(1);
    button('Save').click();
    await settle();
    expect(api.validateSource).toHaveBeenCalledTimes(2);
    expect(api.validateSource).toHaveBeenLastCalledWith('a.sysml', 'package A {\n  part x;\n  part y;\n}\nz');
  });
});

// @verifies CT-USE-009, CT-USE-008
describe('TextEditor and changes on disk', () => {
  // @verifies CT-USE-009
  it('reloads its text from disk and keeps the cursor line', async () => {
    const editor = await open({ line: 3 });
    vi.mocked(api.sourceText).mockResolvedValue({ path: 'a.sysml', text: 'package B {\n  part x;\n  part z;\n}\n' });
    await editor.reloadFromDisk();
    await settle();
    const view = editor.view()!;
    expect(view.state.doc.toString()).toContain('package B');
    expect(view.state.doc.lineAt(view.state.selection.main.head).number).toBe(3);
    expect(document.body.textContent).not.toContain('Unsaved changes');
  });

  // @verifies CT-USE-009, CT-USE-008
  it('saves nothing while the user is asked which text to keep', async () => {
    const editor = await open({ autosaveMs: 20 });
    vi.mocked(api.saveSource).mockResolvedValue({ path: 'a.sysml', bytes: 1 });
    const view = editor.view()!;
    type(view, 'mine');
    editor.hold();
    view.contentDOM.dispatchEvent(new FocusEvent('blur'));
    type(view, '!');
    await wait(50);
    await settle();
    expect(api.saveSource).not.toHaveBeenCalled();
    expect(document.body.textContent).not.toContain('could not');
    // Keeping the edits saves them over the disk version after the pause.
    editor.release(true);
    await wait(50);
    await settle();
    expect(api.saveSource).toHaveBeenCalledTimes(1);
    expect(vi.mocked(api.saveSource).mock.calls[0][1].endsWith('mine!')).toBe(true);
  });

  // @verifies CT-USE-009
  it('takes the disk text after the hold when the user chooses it', async () => {
    const editor = await open({ autosaveMs: 20 });
    type(editor.view()!, 'mine');
    editor.hold();
    vi.mocked(api.sourceText).mockResolvedValue({ path: 'a.sysml', text: 'package Theirs;\n' });
    editor.release(false);
    await editor.reloadFromDisk();
    await wait(50);
    await settle();
    expect(editor.view()!.state.doc.toString()).toBe('package Theirs;\n');
    expect(api.saveSource).not.toHaveBeenCalled();
  });
});
// @end
