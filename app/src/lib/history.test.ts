// @begin CT-TRC-005, CT-EFF-004
import { describe, expect, it } from 'vitest';
import { NavHistory } from './history';

describe('NavHistory', () => {
  // @verifies CT-TRC-005
  it('goes back and forward through the elements visited', () => {
    const h = new NavHistory();
    expect(h.current).toBeUndefined();
    expect(h.back()).toBeUndefined();
    h.visit(1);
    h.visit(2);
    h.visit(3);
    expect(h.canBack).toBe(true);
    expect(h.canForward).toBe(false);
    expect(h.back()).toBe(2);
    expect(h.back()).toBe(1);
    expect(h.canBack).toBe(false);
    expect(h.forward()).toBe(2);
    expect(h.current).toBe(2);
  });

  // @verifies CT-TRC-005
  it('drops the forward trail on a new visit and ignores a repeat of the current element', () => {
    const h = new NavHistory();
    h.visit(1);
    h.visit(2);
    h.back();
    h.visit(9);
    expect(h.canForward).toBe(false);
    h.visit(9);
    expect(h.back()).toBe(1);
  });

  // @verifies CT-EFF-004
  it('keeps at most 100 entries', () => {
    const h = new NavHistory();
    for (let i = 0; i < 500; i++) h.visit(i);
    let steps = 0;
    while (h.back() !== undefined) steps++;
    expect(steps).toBe(99);
  });

  // @verifies CT-TRC-005
  it('starts again when cleared', () => {
    const h = new NavHistory();
    h.visit(1);
    h.visit(2);
    h.clear();
    expect(h.current).toBeUndefined();
    expect(h.canBack).toBe(false);
  });
});
// @end
