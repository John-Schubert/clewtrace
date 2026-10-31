// The text the detail view and the problems list show, built from the rows the Rust side
// sends. Plain functions, so components stay thin.
import type { ElementDetail, ProblemRow, TreeRow } from './api';

// @implements CT-EFF-005, CT-USE-007
/** What the main view details: an element from the tree, or a problem from the list. */
export type Selection = { kind: 'element'; id: number } | { kind: 'problem'; problem: ProblemRow };

// @implements CT-EFF-005
/** The line a tree row shows: the short ID if any, then the name. */
export function treeLabel(row: TreeRow): string {
  const label = [row.shortName, row.name].filter((part) => part).join(' ');
  return label === '' ? '(unnamed)' : label;
}

// @implements CT-EFF-005
export interface Field {
  label: string;
  value: string;
}

// @implements CT-EFF-005
/** What the detail view lists for an element, in order. */
export function detailFields(d: ElementDetail): Field[] {
  const fields: Field[] = [
    { label: 'Kind', value: d.kind },
    { label: 'Qualified name', value: d.qualifiedName },
  ];
  const r = d.requirement;
  if (r) {
    fields.push({ label: 'Doc', value: r.doc });
    if (r.verificationMethod) {
      fields.push({ label: 'Verification method', value: r.verificationMethod });
    }
    if (r.phase !== null) {
      fields.push({ label: 'Phase', value: String(r.phase) });
    }
    const gaps = [
      !r.hasParent && 'no parent',
      !r.implemented && 'no implementation',
      !r.verified && 'no verification',
    ].filter((gap): gap is string => gap !== false);
    fields.push({ label: 'Coverage', value: gaps.length > 0 ? gaps.join(', ') : 'complete' });
  }
  fields.push(
    { label: 'File', value: `${d.file}:${d.line}:${d.column}` },
    { label: 'Links', value: `${d.linksFrom} out, ${d.linksTo} in` },
  );
  return fields;
}

// @implements CT-USE-007
/** One problem as a sentence: where, what went wrong, and how to fix it. */
export function problemSummary(p: ProblemRow): string {
  const place = p.where ? `${p.where}: ` : '';
  return `${place}${p.severity}: ${p.what}. Fix: ${p.help}`;
}
