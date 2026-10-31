// Window math for the shared virtual list, kept apart from the component so it is plain
// TypeScript.

// @implements CT-EFF-005
export interface RangeInput {
  scrollTop: number;
  viewport: number;
  rowHeight: number;
  count: number;
  overscan: number;
}

// @implements CT-EFF-005
/** Rows `start` up to (not including) `end` are rendered, `offset` pixels from the top. */
export interface Range {
  start: number;
  end: number;
  offset: number;
  total: number;
}

// @implements CT-EFF-005
export function visibleRange({ scrollTop, viewport, rowHeight, count, overscan }: RangeInput): Range {
  const total = count * rowHeight;
  if (count === 0) {
    return { start: 0, end: 0, offset: 0, total };
  }
  const first = Math.floor(scrollTop / rowHeight);
  const last = Math.ceil((scrollTop + viewport) / rowHeight);
  const start = Math.min(Math.max(0, first - overscan), count - 1);
  const end = Math.min(count, Math.max(start + 1, last + overscan));
  return { start, end, offset: start * rowHeight, total };
}

// @implements CT-USE-004
/** The active row after a navigation key, or -1 for an empty list. Other keys change nothing. */
export function moveIndex(index: number, key: string, count: number, page: number): number {
  if (count === 0) {
    return -1;
  }
  const last = count - 1;
  switch (key) {
    case 'ArrowDown':
      return Math.min(last, Math.max(0, index + 1));
    case 'ArrowUp':
      return Math.max(0, index - 1);
    case 'PageDown':
      return Math.min(last, Math.max(0, index) + page);
    case 'PageUp':
      return Math.max(0, index - page);
    case 'Home':
      return 0;
    case 'End':
      return last;
    default:
      return index;
  }
}
