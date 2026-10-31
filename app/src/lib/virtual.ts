// Window arithmetic for the shared virtual list. Only the rows in view are in the page.

// @implements CT-EFF-005
export interface RangeInput {
  count: number;
  rowHeight: number;
  viewport: number;
  scrollTop: number;
  overscan: number;
}

// @implements CT-EFF-005
export interface VisibleRange {
  /** First row to render. */
  start: number;
  /** One past the last row to render. */
  end: number;
  /** Pixels above the first rendered row. */
  offset: number;
  totalHeight: number;
}

// @implements CT-EFF-005
export function visibleRange(input: RangeInput): VisibleRange {
  const { count, rowHeight, viewport, scrollTop, overscan } = input;
  if (count <= 0) {
    return { start: 0, end: 0, offset: 0, totalHeight: 0 };
  }
  const first = Math.floor(scrollTop / rowHeight);
  const last = Math.ceil((scrollTop + viewport) / rowHeight);
  const start = Math.min(Math.max(0, first - overscan), count - 1);
  const end = Math.min(count, last + overscan);
  return { start, end, offset: start * rowHeight, totalHeight: count * rowHeight };
}

// @implements CT-USE-004
/** The smallest scroll position that brings a row fully into view (keyboard navigation). */
export function scrollTopFor(input: {
  index: number;
  rowHeight: number;
  viewport: number;
  scrollTop: number;
}): number {
  const { index, rowHeight, viewport, scrollTop } = input;
  const top = index * rowHeight;
  const bottom = top + rowHeight;
  if (top < scrollTop) {
    return top;
  }
  if (bottom > scrollTop + viewport) {
    return bottom - viewport;
  }
  return scrollTop;
}
