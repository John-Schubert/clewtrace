// @begin CT-TRC-004, CT-EFF-006
// Tests for the labels and targets of the coverage, search, and source views.
import { describe, expect, it } from 'vitest';
import type { GapRow, SearchHit } from './api';
import { codeTarget, gapLabel, GAP_FILTERS, filterLabel, hitLabel, hitTarget, sourceLabel } from './find';

const gap: GapRow = { id: 3, kind: 'requirement', name: 'lone', shortName: 'X-004', qualifiedName: 'Reqs::lone', file: 'r.sysml', line: 7 };
const hit = (over: Partial<SearchHit> = {}): SearchHit => ({
  id: 5,
  kind: 'requirement',
  qualifiedName: 'Reqs::top',
  shortName: 'X-001',
  file: 'r.sysml',
  line: 3,
  column: 1,
  ...over,
});

describe('coverage filters', () => {
  // @verifies CT-TRC-004
  it('lists the four gaps with the names the Rust side accepts', () => {
    expect(GAP_FILTERS.map((f) => f.id)).toEqual(['no-parent', 'no-implementation', 'no-verification', 'untraced']);
  });

  // @verifies CT-TRC-004
  it('labels a filter with its count', () => {
    expect(filterLabel(GAP_FILTERS[0], { 'no-parent': 12 })).toBe('No parent (12)');
    expect(filterLabel(GAP_FILTERS[1], undefined)).toBe('No implementation');
  });

  // @verifies CT-TRC-004
  it('labels a row by its ID and qualified name, or the name alone', () => {
    expect(gapLabel(gap)).toBe('X-004 Reqs::lone');
    expect(gapLabel({ ...gap, shortName: null })).toBe('Reqs::lone');
  });
});

describe('search hits', () => {
  // @verifies CT-EFF-006
  it('labels a hit by kind, ID, and qualified name', () => {
    expect(hitLabel(hit())).toBe('requirement X-001 Reqs::top');
    expect(hitLabel(hit({ shortName: null }))).toBe('requirement Reqs::top');
  });

  // @verifies CT-EFF-006, CT-TRC-004
  it('selects the element of a hit, or opens its file when the model has no element', () => {
    expect(hitTarget(hit())).toEqual({ kind: 'element', id: 5 });
    expect(hitTarget(hit({ id: null }))).toEqual({ kind: 'source', path: 'r.sysml', line: 3 });
  });
});

describe('source view', () => {
  // @verifies CT-TRC-004
  it('opens a code end at its first line, and gives nothing for an element', () => {
    expect(codeTarget({ path: 'src/a.rs', startLine: 3, endLine: 9 })).toEqual({ path: 'src/a.rs', line: 3 });
    expect(codeTarget({ id: 1, kind: 'part', name: 'P' })).toBeUndefined();
  });

  // @verifies CT-TRC-004
  it('shows the line number and the text', () => {
    expect(sourceLabel({ line: 12, text: 'fn a() {}' })).toBe('12  fn a() {}');
  });
});
// @end
