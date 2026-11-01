// @begin CT-TRC-005, CT-TRC-006
import { describe, expect, it } from 'vitest';
import type { ImpactRow, LinkRow } from './api';
import { endLabel, followable, impactLabel, linkLabel } from './trace';

const element: LinkRow = { kind: 'satisfies', other: { id: 4, kind: 'part', name: 'Sys::Engine' }, file: 'd.sysml', line: 7 };
const code: LinkRow = { kind: 'implements', other: { path: 'src/a.rs', startLine: 3, endLine: 9 }, file: 'src/a.rs', line: 3 };

describe('trace labels', () => {
  // @verifies CT-TRC-006
  it('names a code end by file and lines', () => {
    expect(endLabel(code.other)).toBe('src/a.rs:3-9');
    expect(endLabel({ path: 'a.rs', startLine: 5, endLine: 5 })).toBe('a.rs:5');
    expect(linkLabel(code)).toBe('implements src/a.rs:3-9');
  });

  // @verifies CT-TRC-005
  it('names an element end by kind and qualified name, and follows only elements', () => {
    expect(linkLabel(element)).toBe('satisfies part Sys::Engine');
    expect(followable(element.other)).toBe(4);
    expect(followable(code.other)).toBeUndefined();
  });

  // @verifies CT-TRC-005
  it('shows how far away an impact row is', () => {
    const row: ImpactRow = { ...element, depth: 3 };
    expect(impactLabel(row)).toBe('3  satisfies part Sys::Engine');
  });
});
// @end
