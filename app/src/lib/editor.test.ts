// @begin CT-MOD-004, CT-USE-003, CT-USE-008, CT-EFF-005
// Tests for the editor's logic. They use the CodeMirror state alone, with no view, so they
// need no layout. The Rust commands are replaced by fakes.
import { CompletionContext } from '@codemirror/autocomplete';
import { redo, undo, undoDepth } from '@codemirror/commands';
import { ensureSyntaxTree } from '@codemirror/language';
import { EditorState } from '@codemirror/state';
import { describe, expect, it, vi } from 'vitest';
import {
  canEdit,
  completeKeywords,
  decideOpen,
  editorState,
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
