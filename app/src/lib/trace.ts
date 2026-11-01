// The text the trace panel shows for links and impact rows. Plain functions, so the
// components stay thin.
import type { ImpactRow, LinkEnd, LinkRow } from './api';

// @implements CT-TRC-006, CT-TRC-005
/** A link end as text: a code scope as `path:start-end`, an element as kind and name. */
export function endLabel(end: LinkEnd): string {
  if ('path' in end) {
    const lines = end.startLine === end.endLine ? `${end.startLine}` : `${end.startLine}-${end.endLine}`;
    return `${end.path}:${lines}`;
  }
  return `${end.kind} ${end.name}`;
}

// @implements CT-TRC-005
/** One link row: its kind, then its far end. */
export function linkLabel(row: LinkRow): string {
  return `${row.kind} ${endLabel(row.other)}`;
}

// @implements CT-TRC-005
/** One impact row: how many links away, then the link that reached it. */
export function impactLabel(row: ImpactRow): string {
  return `${row.depth}  ${linkLabel(row)}`;
}

// @implements CT-TRC-005
/** The element a click follows, or undefined for code, which has no element to select. */
export function followable(end: LinkEnd): number | undefined {
  return 'id' in end ? end.id : undefined;
}
