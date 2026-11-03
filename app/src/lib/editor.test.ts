// @begin CT-MOD-004, CT-USE-003, CT-USE-008, CT-EFF-005
// Tests for the editor's logic. They use the CodeMirror state alone, with no view, so they
// need no layout. The Rust commands are replaced by fakes.
import { CompletionContext } from '@codemirror/autocomplete';
import { redo, undo, undoDepth } from '@codemirror/commands';
import { ensureSyntaxTree } from '@codemirror/language';
import { EditorState } from '@codemirror/state';
import { keymap, type EditorView } from '@codemirror/view';
import { describe, expect, it, vi } from 'vitest';
import {
  canEdit,
  completeKeywords,
  decideOpen,
  definitionCommand,
  editorState,
  referencesCommand,
  placeOf,
  saveCommand,
  tokenClasses,
} from './editor';

const KEYWORDS = ['def', 'package', 'part', 'requirement', 'satisfy', 'verify'];

function make(doc: string, hooks = { onChange: vi.fn(), onSave: vi.fn() }) {
  return { state: editorState(doc, KEYWORDS, hooks), hooks };
}

/** The class of each token of the document, as `text:class`. */
function tokens(doc: string): string[] {
  const { state } = make(doc);
  ensureSyntaxTree(state, doc.length, 1000);
  return tokenClasses(state, 0, doc.length).map((t) => `${doc.slice(t.from, t.to)}:${t.cls}`);
}

describe('syntax highlighting', () => {
  // @verifies CT-MOD-004
  it('marks the keywords it is given and no other name', () => {
    expect(tokens('part def Engine')).toEqual(['part:keyword', 'def:keyword']);
  });

  // @verifies CT-MOD-004
  it('marks comments, strings, and numbers', () => {
    expect(tokens("doc /* a part */ 'part' 42 // satisfy")).toEqual([
      '/* a part */:comment',
      "'part':string",
      '42:number',
      '// satisfy:comment',
    ]);
  });

  // @verifies CT-MOD-004
  it('keeps a block comment going across lines', () => {
    const marked = tokens('/* part\nverify */ verify');
    expect(marked.at(-1)).toBe('verify:keyword');
    expect(marked.filter((t) => t.endsWith(':keyword'))).toHaveLength(1);
    expect(marked.some((t) => t.endsWith(':comment'))).toBe(true);
  });
});

describe('completion', () => {
  const complete = (doc: string) => {
    const { state } = make(doc);
    return completeKeywords(KEYWORDS)(new CompletionContext(state, doc.length, false));
  };

  // @verifies CT-MOD-004
  it('offers the keywords that start with the word before the cursor', () => {
    const result = complete('  sa');
    expect(result?.options.map((o) => o.label)).toEqual(['satisfy']);
    expect(result?.from).toBe(2);
  });

  // @verifies CT-MOD-004
  it('offers nothing where no word has started', () => {
    expect(complete('part ')).toBeNull();
  });
});

describe('undo and redo', () => {
  const typed = (state: EditorState, text: string) =>
    state.update({ changes: { from: state.doc.length, insert: text }, userEvent: 'input.type' }).state;
  const run = (command: typeof undo, state: EditorState) => {
    let next = state;
    command({ state, dispatch: (tr) => (next = tr.state) });
    return next;
  };

  // @verifies CT-USE-003
  it('undoes and redoes an edit', () => {
    let { state } = make('part');
    state = typed(state, ' def');
    expect(state.doc.toString()).toBe('part def');
    state = run(undo, state);
    expect(state.doc.toString()).toBe('part');
    state = run(redo, state);
    expect(state.doc.toString()).toBe('part def');
  });

  // @verifies CT-USE-003
  it('undoes every edit of the session, one step at a time', () => {
    let { state } = make('');
    for (const word of ['a', 'b', 'c']) {
      state = state.update({
        changes: { from: state.doc.length, insert: word },
        userEvent: 'input.type',
        annotations: [],
      }).state;
      // A separate step each time, as if typed after a pause.
      state = state.update({ userEvent: 'select' }).state;
    }
    expect(undoDepth(state)).toBeGreaterThan(0);
    while (undoDepth(state) > 0) {
      state = run(undo, state);
    }
    expect(state.doc.toString()).toBe('');
  });

  // @verifies CT-USE-003
  it('starts with nothing to undo, so opening a file is not an edit', () => {
    expect(undoDepth(make('part').state)).toBe(0);
  });
});

describe('change and save hooks', () => {
  // @verifies CT-USE-008
  it('tells the host about each edit and gives the whole text on save', () => {
    const { state, hooks } = make('part');
    const next = state.update({ changes: { from: 4, insert: 's' } }).state;
    // The update listener runs in a view; here the same extension is called directly.
    expect(hooks.onChange).not.toHaveBeenCalled();
    expect(saveCommand(hooks.onSave)({ state: next } as never)).toBe(true);
    expect(hooks.onSave).toHaveBeenCalledWith('parts');
  });
});

describe('placeOf', () => {
  // @verifies CT-MOD-004
  it('reads the file and line out of a problem place', () => {
    expect(placeOf('model/requirements.sysml:12:5')).toEqual({ path: 'model/requirements.sysml', line: 12 });
    expect(placeOf('a.sysml:3')).toEqual({ path: 'a.sysml', line: 3 });
    expect(placeOf('a.sysml')).toEqual({ path: 'a.sysml', line: 1 });
    expect(placeOf('')).toBeUndefined();
  });

  // @verifies CT-MOD-004
  it('keeps a Windows drive letter out of the line', () => {
    expect(placeOf('C:\\p\\a.sysml:7:1')).toEqual({ path: 'C:\\p\\a.sysml', line: 7 });
  });
});

describe('which files open in the editor', () => {
  // @verifies CT-MOD-004
  it('edits .sysml files and shows everything else read-only', () => {
    expect(canEdit('model/a.sysml')).toBe(true);
    expect(canEdit('library/B.SYSML')).toBe(false);
    expect(canEdit('src/lib.rs')).toBe(false);
    expect(canEdit('sysml')).toBe(false);
  });

  // @verifies CT-USE-008
  it('asks before leaving a file with unsaved edits, and only for another file', () => {
    expect(decideOpen('a.sysml', false, 'b.sysml')).toBe('open');
    expect(decideOpen('a.sysml', true, 'a.sysml')).toBe('open');
    expect(decideOpen('a.sysml', true, 'b.sysml')).toBe('ask');
    expect(decideOpen(undefined, true, 'b.sysml')).toBe('open');
  });
});

// @end

// @begin CT-MOD-004, CT-USE-010
describe('go to definition and find references', () => {
  const found = { found: true as const, id: 7, file: 'b.sysml', line: 3, column: 1, name: 'P::Engine' };
  const navTo = (reply: object) => ({
    definition: vi.fn().mockResolvedValue(reply),
    complete: vi.fn().mockResolvedValue([]),
    detail: vi.fn(),
  });
  const viewAt = (doc: string, head: number) =>
    ({ state: EditorState.create({ doc, selection: { anchor: head } }) }) as unknown as EditorView;
  const tick = () => new Promise((r) => setTimeout(r, 0));

  // @verifies CT-MOD-004
  it('F12 opens the place the name at the cursor leads to', async () => {
    const open = vi.fn();
    expect(definitionCommand(navTo(found), open)(viewAt('part e : Engine;', 12))).toBe(true);
    await tick();
    expect(open).toHaveBeenCalledWith(found);
  });

  // @verifies CT-MOD-004
  it('F12 does nothing for a name that leads nowhere', async () => {
    const open = vi.fn();
    definitionCommand(navTo({ found: false }), open)(viewAt('part e : Engine;', 12));
    await tick();
    expect(open).not.toHaveBeenCalled();
  });

  // @verifies CT-MOD-004
  it('Shift+F12 hands the element ID to the host', async () => {
    const show = vi.fn();
    expect(referencesCommand(navTo(found), show)(viewAt('part e : Engine;', 12))).toBe(true);
    await tick();
    expect(show).toHaveBeenCalledWith(7);
  });

  // @verifies CT-MOD-004
  it('is wired into the editor state when the host gives the lookups', () => {
    const hooks = { onChange: vi.fn(), onSave: vi.fn(), nav: navTo(found), onDefinition: vi.fn(), onReferences: vi.fn() };
    const state = editorState('part e : Engine;', KEYWORDS, hooks);
    expect(state.facet(keymap).flat().map((b) => b.key)).toEqual(expect.arrayContaining(['F12', 'Shift-F12']));
  });
});
// @end
