// @begin CT-MOD-003, CT-EFF-005, CT-EFF-008
// Tests for live validation's logic: the one-shot quiet timer and the markers.
import { Text } from '@codemirror/state';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ClewError, type DiagnosticRow } from './api';
import { markers, QUIET_MS, QuietTimer, stillThere } from './live';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('QuietTimer', () => {
  // @verifies CT-MOD-003
  it('runs once, after the pause', () => {
    const run = vi.fn();
    const timer = new QuietTimer();
    timer.schedule(run);
    vi.advanceTimersByTime(QUIET_MS - 1);
    expect(run).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(run).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(QUIET_MS * 10);
    expect(run).toHaveBeenCalledTimes(1);
  });

  // @verifies CT-MOD-003
  it('is reset by each keystroke, so a burst of typing runs once', () => {
    const run = vi.fn();
    const timer = new QuietTimer(100);
    for (let i = 0; i < 5; i++) {
      timer.schedule(run);
      vi.advanceTimersByTime(60);
    }
    expect(run).not.toHaveBeenCalled();
    vi.advanceTimersByTime(100);
    expect(run).toHaveBeenCalledTimes(1);
  });

  // @verifies CT-EFF-008
  it('leaves no timer behind: it is one-shot, never a poll', () => {
    const timer = new QuietTimer(100);
    timer.schedule(() => {});
    expect(vi.getTimerCount()).toBe(1);
    expect(timer.pending).toBe(true);
    vi.advanceTimersByTime(100);
    expect(vi.getTimerCount()).toBe(0);
    expect(timer.pending).toBe(false);
  });

  // @verifies CT-EFF-008
  it('can be cancelled, and cancelling twice is harmless', () => {
    const run = vi.fn();
    const timer = new QuietTimer(100);
    timer.schedule(run);
    timer.cancel();
    timer.cancel();
    vi.advanceTimersByTime(1000);
    expect(run).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });
});

const row = (over: Partial<DiagnosticRow>): DiagnosticRow => ({
  severity: 'error',
  what: 'cannot find `nope`',
  where: 'a.sysml:2:11',
  help: 'Declare it or fix the name.',
  line: 2,
  column: 11,
  ...over,
});

describe('markers', () => {
  const doc = Text.of(['package A {', '  satisfy Reqs::nope;', '}']);

  // @verifies CT-MOD-003
  it('puts a marker on the word at the line and column', () => {
    const [m] = markers(doc, [row({})]);
    const at = doc.line(2).from + 10;
    expect(m.from).toBe(at);
    expect(doc.sliceString(m.from, m.to)).toBe('Reqs');
    expect(m.severity).toBe('error');
  });

  // @verifies CT-MOD-003, CT-USE-007
  it('carries what went wrong and how to fix it, as text and as a rendered message', () => {
    const [m] = markers(doc, [row({})]);
    expect(m.message).toContain('cannot find `nope`');
    expect(m.message).toContain('Declare it or fix the name.');
    const node = m.renderMessage!({} as never);
    expect(node.textContent).toContain('cannot find `nope`');
    expect(node.textContent).toContain('Declare it or fix the name.');
  });

  // @verifies CT-MOD-003
  it('marks one character when the column is not on a word', () => {
    const [m] = markers(doc, [row({ column: 10, line: 2 })]);
    expect(doc.sliceString(m.from, m.to)).toHaveLength(1);
  });

  // @verifies CT-MOD-003
  it('keeps a warning a warning', () => {
    const [m] = markers(doc, [row({ severity: 'warning' })]);
    expect(m.severity).toBe('warning');
  });

  // @verifies CT-MOD-003
  it('clamps a place past the end of the line or the text, and drops a missing line', () => {
    const past = markers(doc, [row({ column: 500 }), row({ line: 99, column: 1 }), row({ line: 0, column: 1 })]);
    expect(past).toHaveLength(1);
    for (const m of past) {
      expect(m.from).toBeGreaterThanOrEqual(0);
      expect(m.to).toBeLessThanOrEqual(doc.length);
      expect(m.to).toBeGreaterThanOrEqual(m.from);
    }
  });
});

describe('stillThere', () => {
  // @verifies CT-MOD-005
  it('is true when the element answers and false when the Rust side says it is gone', async () => {
    expect(await stillThere(1, async () => ({}))).toBe(true);
    const gone = async () => {
      throw new ClewError({ what: 'no element has ID 1', where: '', help: 'Pick it again' });
    };
    expect(await stillThere(1, gone)).toBe(false);
  });

  // @verifies CT-MOD-005
  it('does not call an element gone because the call itself failed', async () => {
    await expect(stillThere(1, async () => Promise.reject(new Error('ipc closed')))).rejects.toThrow('ipc closed');
  });
});
// @end
