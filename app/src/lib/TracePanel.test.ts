// @begin CT-TRC-005, CT-TRC-006, CT-EFF-005
// Tests for the trace panel. The Rust commands are replaced by fakes.
import { flushSync, mount, unmount } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('./api', async (original) => {
  const real = (await original()) as object;
  return { ...real, elementLinks: vi.fn(), elementImpact: vi.fn() };
});

import * as api from './api';
import TracePanel from './TracePanel.svelte';

let instance: Record<string, unknown> | undefined;
afterEach(() => {
  if (instance) unmount(instance);
  instance = undefined;
  document.body.innerHTML = '';
  vi.clearAllMocks();
});

const settle = async () => {
  for (let i = 0; i < 5; i++) {
    await Promise.resolve();
    await new Promise((r) => setTimeout(r, 0));
    flushSync();
  }
};

const page = <T,>(rows: T[]) => ({ offset: 0, total: rows.length, more: false, rows });
const props = (over: object = {}) => ({
  id: 1,
  canBack: false,
  canForward: false,
  onfollow: vi.fn(),
  onback: vi.fn(),
  onforward: vi.fn(),
  ...over,
});
const options = () => [...document.querySelectorAll('[role="option"]')] as HTMLElement[];

describe('TracePanel', () => {
  // @verifies CT-TRC-005, CT-TRC-006
  it('lists links to and from the element, including code with file and line', async () => {
    vi.mocked(api.elementLinks).mockImplementation(async (_id, direction) =>
      page(
        direction === 'from'
          ? [{ kind: 'derives', other: { id: 2, kind: 'requirement', name: 'R::top' }, file: 'r.sysml', line: 5 }]
          : [{ kind: 'implements', other: { path: 'src/a.rs', startLine: 3, endLine: 9 }, file: 'src/a.rs', line: 3 }],
      ),
    );
    instance = mount(TracePanel, { target: document.body, props: props() });
    await settle();
    const text = options().map((o) => o.textContent?.trim());
    expect(text).toContain('derives requirement R::top');
    expect(text).toContain('implements src/a.rs:3-9');
  });

  // @verifies CT-TRC-005
  it('follows an element link on click and does not follow code', async () => {
    vi.mocked(api.elementLinks).mockResolvedValue(
      page([
        { kind: 'derives', other: { id: 2, kind: 'requirement', name: 'R::top' }, file: 'r.sysml', line: 5 },
        { kind: 'implements', other: { path: 'src/a.rs', startLine: 3, endLine: 9 }, file: 'src/a.rs', line: 3 },
      ]),
    );
    const p = props();
    instance = mount(TracePanel, { target: document.body, props: p });
    await settle();
    options()[0].click();
    expect(p.onfollow).toHaveBeenCalledWith(2);
    options()[1].click();
    expect(p.onfollow).toHaveBeenCalledTimes(1);
  });

  // @verifies CT-TRC-005
  it('shows upstream and downstream impact on the Impact tab', async () => {
    vi.mocked(api.elementLinks).mockResolvedValue(page([]));
    vi.mocked(api.elementImpact).mockImplementation(async (_id, direction) =>
      page([{ depth: 2, kind: 'satisfies', other: { id: 8, kind: 'part', name: `P::${direction}` }, file: 'd.sysml', line: 1 }]),
    );
    instance = mount(TracePanel, { target: document.body, props: props() });
    await settle();
    const tab = [...document.querySelectorAll('button')].find((b) => b.textContent === 'Impact') as HTMLElement;
    tab.click();
    await settle();
    const text = options().map((o) => o.textContent?.trim());
    expect(text).toContain('2  satisfies part P::upstream');
    expect(text).toContain('2  satisfies part P::downstream');
  });

  // @verifies CT-TRC-005
  it('has back and forward buttons that follow the history', async () => {
    vi.mocked(api.elementLinks).mockResolvedValue(page([]));
    const p = props({ canBack: true, canForward: false });
    instance = mount(TracePanel, { target: document.body, props: p });
    await settle();
    const button = (name: string) => [...document.querySelectorAll('button')].find((b) => b.textContent === name) as HTMLButtonElement;
    expect(button('Back').disabled).toBe(false);
    expect(button('Forward').disabled).toBe(true);
    button('Back').click();
    expect(p.onback).toHaveBeenCalled();
  });

  // @verifies CT-USE-007
  it('says what went wrong when links cannot load', async () => {
    vi.mocked(api.elementLinks).mockRejectedValue(new api.ClewError({ what: 'no project is open', where: '', help: 'Open a project folder' }));
    instance = mount(TracePanel, { target: document.body, props: props() });
    await settle();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('no project is open');
  });

  // @verifies CT-EFF-005
  it('shows the links of the selected element within 100 ms of the answer', async () => {
    vi.mocked(api.elementLinks).mockResolvedValue(page([{ kind: 'derives', other: { id: 2, kind: 'requirement', name: 'R::top' }, file: 'r.sysml', line: 5 }]));
    const started = performance.now();
    instance = mount(TracePanel, { target: document.body, props: props() });
    await settle();
    expect(options().length).toBeGreaterThan(0);
    expect(performance.now() - started).toBeLessThan(100);
  });
});
// @end
