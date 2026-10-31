// @begin CT-USE-007, CT-EFF-005
// Tests for the text the detail view and the problems list show.
import { describe, expect, it } from 'vitest';
import type { ElementDetail, ProblemRow, TreeRow } from './api';
import { detailFields, problemSummary, treeLabel } from './describe';

const detail = (extra: Partial<ElementDetail> = {}): ElementDetail => ({
  id: 7, kind: 'requirement usage', name: 'fast', shortName: 'CT-EFF-001', qualifiedName: 'Reqs::fast',
  file: 'model/requirements.sysml', line: 12, column: 5, owner: 2, children: 0, linksFrom: 1, linksTo: 3,
  requirement: null, ...extra,
});

describe('treeLabel', () => {
  // @verifies CT-EFF-005
  it('shows the short ID, the name, and a placeholder for an unnamed element', () => {
    const row = (name: string | null, shortName: string | null): TreeRow =>
      ({ id: 1, kind: 'part', name, shortName, children: 0, file: 'a', line: 1 });
    expect(treeLabel(row('fast', 'CT-EFF-001'))).toBe('CT-EFF-001 fast');
    expect(treeLabel(row('fast', null))).toBe('fast');
    expect(treeLabel(row(null, null))).toBe('(unnamed)');
  });
});

describe('detailFields', () => {
  // @verifies CT-EFF-005
  it('lists the kind, qualified name, file and line, and link counts', () => {
    const f = Object.fromEntries(detailFields(detail()).map((x) => [x.label, x.value]));
    expect(f['Kind']).toBe('requirement usage');
    expect(f['Qualified name']).toBe('Reqs::fast');
    expect(f['File']).toBe('model/requirements.sysml:12:5');
    expect(f['Links']).toBe('1 out, 3 in');
  });

  // @verifies CT-EFF-005
  it('adds the doc and attributes of a requirement, and says what is missing', () => {
    const f = Object.fromEntries(detailFields(detail({
      requirement: { doc: 'Be fast.', verificationMethod: 'Test', phase: 2, hasParent: true, implemented: false, verified: true, waived: false },
    })).map((x) => [x.label, x.value]));
    expect(f['Doc']).toBe('Be fast.');
    expect(f['Verification method']).toBe('Test');
    expect(f['Phase']).toBe('2');
    expect(f['Coverage']).toBe('no implementation');
  });
});

describe('problemSummary', () => {
  const p: ProblemRow = { severity: 'error', what: 'unknown name `X`', where: 'a.sysml:3:9', help: 'Define X or fix the spelling' };
  // @verifies CT-USE-007
  it('states where, what, and how to fix it', () => {
    expect(problemSummary(p)).toBe('a.sysml:3:9: error: unknown name `X`. Fix: Define X or fix the spelling');
    expect(problemSummary({ ...p, where: '' })).toBe('error: unknown name `X`. Fix: Define X or fix the spelling');
  });
});
// @end
