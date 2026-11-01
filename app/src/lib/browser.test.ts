// @begin CT-MOD-005, CT-EFF-005, CT-USE-007
// Tests for the model browser, the detail view, and the problems panel. The Rust commands
// are replaced by fakes, so no project and no Tauri are needed.
import { flushSync, mount, unmount } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('./api', async (original) => {
  const real = (await original()) as object;
  return { ...real, treeChildren: vi.fn(), elementDetail: vi.fn(), problems: vi.fn() };
});

import * as api from './api';
import DetailView from './DetailView.svelte';
import ModelBrowser from './ModelBrowser.svelte';
import ProblemsPanel from './ProblemsPanel.svelte';

let instance: Record<string, unknown> | undefined;
afterEach(() => {
  if (instance) {
    unmount(instance);
    instance = undefined;
  }
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

const row = (id: number, name: string, children = 0): api.TreeRow =>
  ({ id, kind: 'package', name, shortName: null, children, file: 'a.sysml', line: id });

describe('ModelBrowser', () => {
  // @verifies CT-MOD-005, CT-EFF-005
  it('shows the root, expands a package with the right arrow, and selects with Enter', async () => {
    vi.mocked(api.treeChildren).mockImplementation(async (parent, offset = 0) => {
      const rows = parent === 0 ? [row(1, 'Reqs', 2), row(2, 'Other')] : [row(11, 'Speed'), row(12, 'Memory')];
      return { offset, total: rows.length, more: false, rows };
    });
    const onselect = vi.fn();
    instance = mount(ModelBrowser, { target: document.body, props: { onselect } });
    await settle();
    const text = () => [...document.querySelectorAll('[role="option"]')].map((o) => o.textContent?.trim());
    expect(text()).toEqual(['▸ Reqs', '  Other'].map((s) => s.trim()));
    const list = document.querySelector('[role="listbox"]') as HTMLElement;
    const key = (k: string) => list.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true }));
    key('ArrowDown');
    key('ArrowRight');
    await settle();
    expect(text()).toEqual(['▾ Reqs', 'Speed', 'Memory', 'Other']);
    key('ArrowDown');
    key('Enter');
    expect(onselect).toHaveBeenLastCalledWith(11);
    key('ArrowLeft');
    await settle();
    expect(text()).toEqual(['▾ Reqs', 'Speed', 'Memory', 'Other']);
  });

  // @verifies CT-USE-007
  it('says what went wrong when the tree cannot load', async () => {
    vi.mocked(api.treeChildren).mockRejectedValue(new api.ClewError({ what: 'no project is open', where: '', help: 'Open a project folder' }));
    instance = mount(ModelBrowser, { target: document.body, props: { onselect: () => {} } });
    await settle();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('no project is open');
  });
});

describe('DetailView', () => {
  // @verifies CT-EFF-005
  it('shows the kind, qualified name, and file and line of the selected element', async () => {
    vi.mocked(api.elementDetail).mockResolvedValue({
      id: 7, kind: 'package', name: 'Reqs', shortName: null, qualifiedName: 'Reqs', file: 'm.sysml', line: 4, column: 1,
      owner: null, children: 2, linksFrom: 0, linksTo: 0, requirement: null,
    });
    instance = mount(DetailView, { target: document.body, props: { selection: { kind: 'element', id: 7 }, onopen: vi.fn() } });
    await settle();
    expect(document.body.textContent).toContain('Qualified name');
    expect(document.body.textContent).toContain('m.sysml:4:1');
  });

  // @verifies CT-TRC-004
  it('opens the file of the selected element in the source view at its line', async () => {
    vi.mocked(api.elementDetail).mockResolvedValue({
      id: 7, kind: 'package', name: 'Reqs', shortName: null, qualifiedName: 'Reqs', file: 'm.sysml', line: 4, column: 1,
      owner: null, children: 2, linksFrom: 0, linksTo: 0, requirement: null,
    });
    const onopen = vi.fn();
    instance = mount(DetailView, { target: document.body, props: { selection: { kind: 'element', id: 7 }, onopen } });
    await settle();
    (document.querySelector('p button') as HTMLButtonElement).click();
    expect(onopen).toHaveBeenCalledWith('m.sysml', 4);
  });

  // @verifies CT-USE-007
  it('shows a selected problem with its place and fix', async () => {
    const problem: api.ProblemRow = { severity: 'warning', what: 'no verification', where: 'r.sysml:9:3', help: 'Add a verify link' };
    instance = mount(DetailView, { target: document.body, props: { selection: { kind: 'problem', problem }, onopen: vi.fn() } });
    await settle();
    expect(document.body.textContent).toContain('r.sysml:9:3');
    expect(document.body.textContent).toContain('Add a verify link');
  });
});

describe('ProblemsPanel', () => {
  // @verifies CT-USE-007, CT-EFF-005
  it('lists each problem with its place and fix, and selects on click', async () => {
    const rows: api.ProblemRow[] = [
      { severity: 'error', what: 'unknown name `X`', where: 'a.sysml:3:9', help: 'Define X' },
      { severity: 'warning', what: 'untraced', where: 'b.rs:1', help: 'Add a tag' },
    ];
    vi.mocked(api.problems).mockResolvedValue({ offset: 0, total: 2, more: false, rows });
    const onselect = vi.fn();
    instance = mount(ProblemsPanel, { target: document.body, props: { onselect } });
    await settle();
    const options = document.querySelectorAll('[role="option"]');
    expect(options.length).toBe(2);
    expect(options[0].textContent).toContain('a.sysml:3:9');
    expect(options[0].textContent).toContain('Define X');
    (options[1] as HTMLElement).click();
    expect(onselect).toHaveBeenCalledWith(rows[1]);
  });

  // @verifies CT-USE-007
  it('says so when there are no problems', async () => {
    vi.mocked(api.problems).mockResolvedValue({ offset: 0, total: 0, more: false, rows: [] });
    instance = mount(ProblemsPanel, { target: document.body, props: { onselect: () => {} } });
    await settle();
    expect(document.body.textContent).toContain('No problems');
  });
});
// @end
