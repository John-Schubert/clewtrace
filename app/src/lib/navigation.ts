// @begin CT-MOD-004, CT-USE-010, CT-EFF-005
// Go to definition, find references, name completion, and the hover in the editor. The
// Rust core owns the model and answers each question for a place in the file; this module
// only finds the name under the cursor and shapes the answers for CodeMirror.
import type { CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import type { EditorState, Text } from '@codemirror/state';
import type { Definition, ElementDetail, NameCompletion } from './api';

/** What the editor asks the host. The host fills in the open file's path. */
export interface EditorNav {
  /** Where the name at a 1-based line and column leads (`word` is the name, unquoted). */
  definition: (line: number, column: number, word: string) => Promise<Definition>;
  /** Names that fit after `before` (the line's text before the word) and `prefix`. */
  complete: (line: number, before: string, prefix: string) => Promise<NameCompletion[]>;
  /** The element's detail, for the hover. */
  detail: (id: number) => Promise<ElementDetail>;
}

/** A name in the text: where it is, and its text without quotes. */
export interface NameAt {
  from: number;
  to: number;
  text: string;
}

/** The identifier or quoted name at the position, or just before it. */
export function nameAt(doc: Text, pos: number): NameAt | undefined {
  const line = doc.lineAt(pos);
  const text = line.text;
  const at = pos - line.from;
  for (const m of text.matchAll(/'[^'\n]*'|[A-Za-z_][A-Za-z0-9_]*/g)) {
    const start = m.index;
    const end = start + m[0].length;
    if (at >= start && at <= end) {
      const quoted = m[0].startsWith("'");
      return { from: line.from + start, to: line.from + end, text: quoted ? m[0].slice(1, -1) : m[0] };
    }
  }
  return undefined;
}

/** Where the name at the position leads, or nothing when it is not a name that leads anywhere. */
export async function definitionAt(state: EditorState, pos: number, nav: EditorNav): Promise<Extract<Definition, { found: true }> | undefined> {
  const name = nameAt(state.doc, pos);
  if (!name) {
    return undefined;
  }
  const line = state.doc.lineAt(name.from);
  const found = await nav.definition(line.number, name.from - line.from + 1, name.text);
  return found.found ? found : undefined;
}

/** The ID of the element whose references to list, for the name at the position. */
export async function referencesAt(state: EditorState, pos: number, nav: EditorNav): Promise<number | undefined> {
  return (await definitionAt(state, pos, nav))?.id;
}

/** The completion type CodeMirror shows an icon for, by the kind the core gives. */
function completionType(kind: string): string {
  if (kind === 'package') {
    return 'namespace';
  }
  return kind.endsWith('def') ? 'class' : 'variable';
}

/** Completion of the names the core says fit at the cursor. A quoted word is replaced with its quotes. */
export function completeNames(nav: EditorNav) {
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    const word = context.matchBefore(/'[^'\n]*|[A-Za-z_][A-Za-z0-9_]*/) ?? (context.explicit ? { from: context.pos, text: '' } : null);
    if (!word) {
      return null;
    }
    const line = context.state.doc.lineAt(word.from);
    const quoted = word.text.startsWith("'");
    const prefix = quoted ? word.text.slice(1) : word.text;
    try {
      const names = await nav.complete(line.number, line.text.slice(0, word.from - line.from), prefix);
      return names.length === 0
        ? null
        : {
            from: word.from,
            options: names.map((n) => ({ label: n.label, detail: n.detail, type: completionType(n.detail), apply: n.insert })),
          };
    } catch {
      return null;
    }
  };
}

/** The lines the hover shows for an element: its kind and name, its doc text, and its trace status. */
export function hoverLines(d: ElementDetail): string[] {
  const lines = [`${d.kind} ${d.qualifiedName}`];
  const r = d.requirement;
  if (!r) {
    return [...lines, `Trace links: ${d.linksFrom} out, ${d.linksTo} in`];
  }
  if (r.doc) {
    lines.push(r.doc);
  }
  const status = [r.hasParent ? 'has a parent' : 'no parent', r.implemented ? 'implemented' : 'not implemented', r.verified ? 'verified' : 'not verified'];
  if (r.waived) {
    status.push('waived');
  }
  return [...lines, `Trace: ${status.join(', ')}`];
}

/** What to show over the name at the position, or null. */
export async function hoverFor(state: EditorState, pos: number, nav: EditorNav): Promise<{ from: number; to: number; lines: string[] } | null> {
  try {
    const name = nameAt(state.doc, pos);
    const target = name && (await definitionAt(state, pos, nav));
    if (!name || !target) {
      return null;
    }
    return { from: name.from, to: name.to, lines: hoverLines(await nav.detail(target.id)) };
  } catch {
    return null;
  }
}
// @end
