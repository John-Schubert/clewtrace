// The text and targets of the coverage, search, and source views. Plain functions, so the
// components stay thin.
import type { GapFilter, GapRow, LinkEnd, SearchHit, SourceLine } from './api';

// @implements CT-TRC-004
/** The coverage filters in the order they are listed, with the label on each button. */
export const GAP_FILTERS: { id: GapFilter; label: string }[] = [
  { id: 'no-parent', label: 'No parent' },
  { id: 'no-implementation', label: 'No implementation' },
  { id: 'no-verification', label: 'No verification' },
  { id: 'untraced', label: 'Untraced' },
];

// @implements CT-TRC-004
/** A filter's label with its count, when the count is known. */
export function filterLabel(filter: { id: GapFilter; label: string }, counts: Partial<Record<GapFilter, number>> | undefined): string {
  const count = counts?.[filter.id];
  return count === undefined ? filter.label : `${filter.label} (${count})`;
}

// @implements CT-TRC-004
/** A coverage row: the requirement ID if it has one, then the qualified name. */
export function gapLabel(row: GapRow): string {
  return [row.shortName, row.qualifiedName].filter((part) => part).join(' ');
}

// @implements CT-EFF-006
/** A search hit: its kind, its ID if it has one, then the qualified name. */
export function hitLabel(hit: SearchHit): string {
  return [hit.kind, hit.shortName, hit.qualifiedName].filter((part) => part).join(' ');
}

// @implements CT-EFF-006, CT-TRC-004
/** What a click on a hit does: select its element, or open its file when the model has no element for it. */
export type HitTarget = { kind: 'element'; id: number } | { kind: 'source'; path: string; line: number };

// @implements CT-EFF-006, CT-TRC-004
export function hitTarget(hit: SearchHit): HitTarget {
  return hit.id === null ? { kind: 'source', path: hit.file, line: hit.line } : { kind: 'element', id: hit.id };
}

// @implements CT-TRC-004
/** The file and line the source view opens for a code end, or undefined for an element. */
export function codeTarget(end: LinkEnd): { path: string; line: number } | undefined {
  return 'path' in end ? { path: end.path, line: end.startLine } : undefined;
}

// @implements CT-TRC-004
/** One line of the source view: its number, then its text. */
export function sourceLabel(row: SourceLine): string {
  return `${row.line}  ${row.text}`;
}
