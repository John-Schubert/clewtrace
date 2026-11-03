// @begin CT-MOD-004, CT-USE-010, CT-EFF-005
// Tests for go to definition, find references, completion, and the hover in the editor.
// They use the CodeMirror state alone, with fakes for the Rust commands.
import { CompletionContext } from '@codemirror/autocomplete';
import { EditorState } from '@codemirror/state';
import { describe, expect, it, vi } from 'vitest';
import type { ElementDetail } from './api';
import { completeNames, definitionAt, hoverFor, hoverLines, nameAt, referencesAt, type EditorNav } from './navigation';

const found = { found: true as const, id: 7, file: 'a.sysml', line: 3, column: 1, name: 'P::Engine' };

const detail = (extra: Partial<ElementDetail> = {}): ElementDetail => ({
  id: 7, kind: 'part def', name: 'Engine', shortName: null, qualifiedName: 'P::Engine',
  file: 'a.sysml', line: 3, column: 1, owner: 2, children: 0, linksFrom: 1, linksTo: 3,
  doc: '', requirement: null, ...extra,
});

function nav(extra: Partial<EditorNav> = {}): EditorNav {
  return {
    definition: vi.fn().mockResolvedValue(found),
    complete: vi.fn().mockResolvedValue([]),
    detail: vi.fn().mockResolvedValue(detail()),
    ...extra,
  };
}

const stateOf = (doc: string) => EditorState.create({ doc });

describe('nameAt', () => {
  // @verifies CT-MOD-004
  it('finds the identifier under or just after the cursor', () => {
    const s = stateOf('  part e : Engine;');
    expect(nameAt(s.doc, 14)).toEqual({ from: 11, to: 17, text: 'Engine' });
    expect(nameAt(s.doc, 17)).toEqual({ from: 11, to: 17, text: 'Engine' });
  });

  // @verifies CT-MOD-004
  it('reads a quoted name whole, hyphens and spaces included, without its quotes', () => {
    const s = stateOf("satisfy 'CT-MOD-004' by e;");
    expect(nameAt(s.doc, 12)).toEqual({ from: 8, to: 20, text: 'CT-MOD-004' });
    expect(nameAt(stateOf("part 'my part';").doc, 7)?.text).toBe('my part');
  });

  // @verifies CT-MOD-004
  it('finds nothing on blanks and punctuation', () => {
    const s = stateOf('a  ;  b');
    expect(nameAt(s.doc, 2)).toBeUndefined();
    expect(nameAt(s.doc, 3)).toBeUndefined();
  });
});

describe('definitionAt', () => {
  // @verifies CT-MOD-004
  it('asks the core with the line, the name start column, and the name', async () => {
    const n = nav();
    const s = stateOf('package P {\n  part e : Engine;\n}');
    const at = s.doc.line(2).from + 14;
    expect(await definitionAt(s, at, n)).toEqual(found);
    expect(n.definition).toHaveBeenCalledWith(2, 12, 'Engine');
  });

  // @verifies CT-MOD-004
  it('gives nothing without asking when the cursor is not on a name, or when the core finds none', async () => {
    const n = nav({ definition: vi.fn().mockResolvedValue({ found: false }) });
    expect(await definitionAt(stateOf('a  b'), 2, n)).toBeUndefined();
    expect(n.definition).not.toHaveBeenCalled();
    expect(await definitionAt(stateOf('abc'), 1, n)).toBeUndefined();
  });
});

describe('referencesAt', () => {
  // @verifies CT-MOD-004
  it('gives the element ID the name leads to', async () => {
    expect(await referencesAt(stateOf('part e : Engine;'), 12, nav())).toBe(7);
    const none = nav({ definition: vi.fn().mockResolvedValue({ found: false }) });
    expect(await referencesAt(stateOf('part e : Engine;'), 12, none)).toBeUndefined();
  });
});

describe('completeNames', () => {
  const run = (doc: string, n: EditorNav, explicit = false) =>
    completeNames(n)(new CompletionContext(stateOf(doc), doc.length, explicit));

  // @verifies CT-MOD-004
  it('sends the text before the word and the word, and maps the names to options', async () => {
    const n = nav({
      complete: vi.fn().mockResolvedValue([
        { label: 'Engine', insert: 'Engine', detail: 'part def' },
        { label: 'P', insert: 'P', detail: 'package' },
        { label: 'e', insert: 'e', detail: 'part' },
      ]),
    });
    const result = await run('a\n  part e : En', n);
    expect(n.complete).toHaveBeenCalledWith(2, '  part e : ', 'En');
    expect(result?.from).toBe(13);
    expect(result?.options.map((o) => [o.label, o.detail, o.type])).toEqual([
      ['Engine', 'part def', 'class'],
      ['P', 'package', 'namespace'],
      ['e', 'part', 'variable'],
    ]);
  });

  // @verifies CT-MOD-004
  it('replaces a quoted requirement ID whole, quotes included', async () => {
    const n = nav({ complete: vi.fn().mockResolvedValue([{ label: 'CT-X-001', insert: "'CT-X-001'", detail: 'requirement' }]) });
    const result = await run("  satisfy 'CT-X", n);
    expect(n.complete).toHaveBeenCalledWith(1, '  satisfy ', 'CT-X');
    expect(result?.from).toBe(10);
    expect(result?.options[0].apply).toBe("'CT-X-001'");
    expect(result?.options[0].label).toBe('CT-X-001');
  });

  // @verifies CT-MOD-004
  it('asks after satisfy only when completion is requested, and offers nothing when the core has no names', async () => {
    const n = nav();
    expect(await run('  satisfy ', n, false)).toBeNull();
    expect(n.complete).not.toHaveBeenCalled();
    expect(await run('  satisfy ', n, true)).toBeNull();
    expect(n.complete).toHaveBeenCalledWith(1, '  satisfy ', '');
  });

  // @verifies CT-MOD-004
  it('offers nothing when the core fails', async () => {
    const n = nav({ complete: vi.fn().mockRejectedValue(new Error('no project')) });
    expect(await run('  part e : En', n)).toBeNull();
  });
});

describe('hover', () => {
  // @verifies CT-USE-010
  it('shows the kind, the doc text, and the trace status of a requirement', () => {
    const r = { doc: 'It shall go.', verificationMethod: 'Test', phase: 2, hasParent: true, implemented: true, verified: false, waived: false };
    expect(hoverLines(detail({ kind: 'requirement', qualifiedName: 'R::go', doc: r.doc, requirement: r }))).toEqual([
      'requirement R::go',
      'It shall go.',
      'Trace: has a parent, implemented, not verified',
    ]);
    const waived = { ...r, hasParent: false, implemented: false, verified: false, waived: true };
    expect(hoverLines(detail({ requirement: waived })).at(-1)).toBe('Trace: no parent, not implemented, not verified, waived');
  });

  // @verifies CT-USE-010
  it('shows the doc text of any element that has some', () => {
    const d = detail({ doc: 'Makes the thrust.' });
    expect(hoverLines(d)).toEqual(['part def P::Engine', 'Makes the thrust.', 'Trace links: 1 out, 3 in']);
  });

  // @verifies CT-USE-010
  it('shows the kind and the link counts of other elements', () => {
    expect(hoverLines(detail())).toEqual(['part def P::Engine', 'Trace links: 1 out, 3 in']);
  });

  // @verifies CT-USE-010
  it('covers the name under the pointer and fetches the element it leads to', async () => {
    const n = nav();
    const tip = await hoverFor(stateOf('part e : Engine;'), 13, n);
    expect(tip).toEqual({ from: 9, to: 15, lines: ['part def P::Engine', 'Trace links: 1 out, 3 in'] });
    expect(n.detail).toHaveBeenCalledWith(7);
  });

  // @verifies CT-USE-010
  it('shows nothing off a name, for an unresolved name, or when the core fails', async () => {
    expect(await hoverFor(stateOf('a  b'), 2, nav())).toBeNull();
    const none = nav({ definition: vi.fn().mockResolvedValue({ found: false }) });
    expect(await hoverFor(stateOf('abc'), 1, none)).toBeNull();
    const broken = nav({ detail: vi.fn().mockRejectedValue(new Error('gone')) });
    expect(await hoverFor(stateOf('abc'), 1, broken)).toBeNull();
  });
});
// @end
