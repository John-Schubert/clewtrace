// @begin CT-MOD-004, CT-USE-003, CT-USE-008, CT-EFF-005
// The text editor for .sysml files: CodeMirror 6 set up with highlighting from the
// lexer's own keyword list, keyword completion, search, undo and redo, and an explicit
// save. The logic lives here so the Svelte component stays thin.
import { autocompletion, completionKeymap, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { indentUnit, StreamLanguage, syntaxTree, type StreamParser } from '@codemirror/language';
import { highlightSelectionMatches, search, searchKeymap } from '@codemirror/search';
import { EditorState, RangeSetBuilder, type Text } from '@codemirror/state';
import {
  Decoration,
  drawSelection,
  EditorView,
  highlightActiveLine,
  keymap,
  lineNumbers,
  ViewPlugin,
  type Command,
  type DecorationSet,
  type ViewUpdate,
} from '@codemirror/view';

/** What the host wants to hear from the editor. */
export interface EditorHooks {
  /** The document changed; the host compares it with the saved text. */
  onChange: (doc: Text) => void;
  /** The user asked to save, with the whole text. */
  onSave: (text: string) => void;
}

/** The source classes the highlighter marks, by the token name the tokenizer returns. */
const TOKEN_CLASSES = ['keyword', 'comment', 'string', 'number'];

/** Whether a file opens in the editor. The Rust side saves `.sysml` files and nothing else. */
export function canEdit(path: string): boolean {
  return path.endsWith('.sysml') && path.length > '.sysml'.length;
}

/** The file and line in a problem place such as `model/a.sysml:12:5`. */
export function placeOf(place: string): { path: string; line: number } | undefined {
  if (!place) {
    return undefined;
  }
  const match = /^(.+?):(\d+)(?::\d+)?$/.exec(place);
  return match ? { path: match[1], line: Number(match[2]) } : { path: place, line: 1 };
}

/**
 * What to do when asked to open `target`: `open` it, or `ask` first because the file now
 * open has edits that are not saved. Moving within the same file never asks.
 */
export function decideOpen(current: string | undefined, dirty: boolean, target: string): 'open' | 'ask' {
  return current !== undefined && dirty && current !== target ? 'ask' : 'open';
}

/** A tokenizer for SysML v2 text. Only the words in `keywords` are keywords. */
function sysmlParser(keywords: ReadonlySet<string>): StreamParser<{ block: boolean }> {
  return {
    name: 'sysml',
    startState: () => ({ block: false }),
    token(stream, state) {
      if (state.block) {
        while (!stream.eol()) {
          if (stream.match('*/')) {
            state.block = false;
            break;
          }
          stream.next();
        }
        return 'comment';
      }
      if (stream.eatSpace()) {
        return null;
      }
      if (stream.match('//')) {
        stream.skipToEnd();
        return 'comment';
      }
      if (stream.match('/*')) {
        state.block = true;
        while (!stream.eol()) {
          if (stream.match('*/')) {
            state.block = false;
            break;
          }
          stream.next();
        }
        return 'comment';
      }
      const quote = stream.peek();
      if (quote === "'" || quote === '"') {
        stream.next();
        let ch: string | void;
        while ((ch = stream.next()) !== undefined) {
          if (ch === '\\') {
            stream.next();
          } else if (ch === quote) {
            break;
          }
        }
        return 'string';
      }
      if (stream.match(/^\d[\d_]*(\.\d+)?([eE][+-]?\d+)?/)) {
        return 'number';
      }
      const word = stream.match(/^[A-Za-z_][A-Za-z0-9_]*/);
      if (word) {
        return keywords.has((word as RegExpMatchArray)[0]) ? 'keyword' : null;
      }
      stream.next();
      return null;
    },
  };
}

/** The highlighted tokens in a range, as the token name and where each one is. */
export function tokenClasses(state: EditorState, from: number, to: number): { from: number; to: number; cls: string }[] {
  const found: { from: number; to: number; cls: string }[] = [];
  syntaxTree(state).iterate({
    from,
    to,
    enter(node) {
      if (TOKEN_CLASSES.includes(node.name) && node.to > node.from) {
        found.push({ from: node.from, to: node.to, cls: node.name });
      }
    },
  });
  return found;
}

/** Marks the visible tokens with a class the editor theme colors. */
function marks(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const { from, to } of view.visibleRanges) {
    for (const t of tokenClasses(view.state, from, to)) {
      builder.add(t.from, t.to, Decoration.mark({ class: `cm-sysml-${t.cls}` }));
    }
  }
  return builder.finish();
}

const highlighter = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = marks(view);
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged || syntaxTree(update.startState) !== syntaxTree(update.state)) {
        this.decorations = marks(update.view);
      }
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

/** Completion of keywords that start with the word before the cursor. */
export function completeKeywords(keywords: readonly string[]) {
  return (context: CompletionContext): CompletionResult | null => {
    const word = context.matchBefore(/[A-Za-z_][A-Za-z0-9_]*/);
    if (!word) {
      return null;
    }
    const options = keywords.filter((k) => k.startsWith(word.text) && k !== word.text).map((label) => ({ label, type: 'keyword' }));
    return options.length > 0 ? { from: word.from, options, validFor: /^[A-Za-z0-9_]*$/ } : null;
  };
}

/** The command for Ctrl+S: hands the whole text to the host. */
export function saveCommand(onSave: (text: string) => void): Command {
  return (view) => {
    onSave(view.state.doc.toString());
    return true;
  };
}

/** The colors come from the app's design tokens, so the editor follows the theme. */
const theme = EditorView.theme({
  '&': { height: '100%', color: 'var(--text)', backgroundColor: 'var(--surface)' },
  '&.cm-focused': { outline: '2px solid var(--focus)' },
  '.cm-scroller': { fontFamily: 'ui-monospace, monospace', overflow: 'auto' },
  '.cm-gutters': { backgroundColor: 'var(--bg)', color: 'var(--text-muted)', border: 'none' },
  '.cm-activeLine': { backgroundColor: 'var(--row-hover)' },
  '.cm-activeLineGutter': { backgroundColor: 'var(--row-hover)' },
  '.cm-cursor': { borderLeftColor: 'var(--text)' },
  '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': { backgroundColor: 'var(--row-selected)' },
  '.cm-selectionMatch': { backgroundColor: 'var(--row-hover)' },
  '.cm-panels': { backgroundColor: 'var(--bg)', color: 'var(--text)', borderColor: 'var(--border)' },
  '.cm-panel input, .cm-panel button': { font: 'inherit', color: 'var(--text)', backgroundColor: 'var(--surface)', border: '1px solid var(--border)' },
  '.cm-tooltip': { backgroundColor: 'var(--surface)', color: 'var(--text)', border: '1px solid var(--border)' },
  '.cm-tooltip-autocomplete ul li[aria-selected]': { backgroundColor: 'var(--row-selected)', color: 'var(--text)' },
  '.cm-sysml-keyword': { color: 'var(--accent)', fontWeight: 'bold' },
  '.cm-sysml-comment': { color: 'var(--text-muted)', fontStyle: 'italic' },
  '.cm-sysml-string': { color: 'var(--text-muted)' },
  '.cm-sysml-number': { color: 'var(--accent)' },
});

/** The editor state for a file's text, with undo history starting empty. */
export function editorState(doc: string, keywords: readonly string[], hooks: EditorHooks): EditorState {
  return EditorState.create({
    doc,
    extensions: [
      lineNumbers(),
      history(),
      drawSelection(),
      highlightActiveLine(),
      indentUnit.of('  '),
      EditorState.tabSize.of(2),
      StreamLanguage.define(sysmlParser(new Set(keywords))),
      highlighter,
      autocompletion({ override: [completeKeywords(keywords)] }),
      search({ top: true }),
      highlightSelectionMatches(),
      keymap.of([
        { key: 'Mod-s', run: saveCommand(hooks.onSave), preventDefault: true },
        ...completionKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...defaultKeymap,
        indentWithTab,
      ]),
      EditorView.contentAttributes.of({ 'aria-label': 'SysML v2 text editor' }),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          hooks.onChange(update.state.doc);
        }
      }),
      theme,
    ],
  });
}

/**
 * Whether this web view can style the editor under the app's content security policy.
 * CodeMirror adds its styles through a constructed style sheet inside a shadow root, which
 * the policy allows. A web view without that cannot show the editor, so the source view
 * is used instead.
 */
export function supportsEditor(): boolean {
  return (
    typeof ShadowRoot !== 'undefined' &&
    'adoptedStyleSheets' in ShadowRoot.prototype &&
    typeof CSSStyleSheet !== 'undefined' &&
    'replaceSync' in CSSStyleSheet.prototype
  );
}

/** Opens an editor view in a shadow root on `host`. */
export function openView(host: HTMLElement, state: EditorState): EditorView {
  const root = host.attachShadow({ mode: 'open' });
  const parent = document.createElement('div');
  parent.style.height = '100%';
  root.append(parent);
  return new EditorView({ state, parent, root });
}

/** Puts the cursor at the start of a line (1-based, clamped) and scrolls it into view. */
export function goToLine(view: EditorView, line: number): void {
  const at = view.state.doc.line(Math.min(Math.max(Math.trunc(line) || 1, 1), view.state.doc.lines));
  view.dispatch({ selection: { anchor: at.from }, effects: EditorView.scrollIntoView(at.from, { y: 'center' }) });
  view.focus();
}
// @end
