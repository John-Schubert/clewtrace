// @begin CT-TRC-004, CT-EFF-005, CT-EFF-006, CT-USE-007
// Tests for the coverage, search, and source views. The Rust commands are replaced by fakes.
import { flushSync, mount, unmount } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('./api', async (original) => {
  const real = (await original()) as object;
  return { ...real, coverageGaps: vi.fn(), search: vi.fn(), sourceLines: vi.fn() };
});

import * as api from './api';
import CoverageView from './CoverageView.svelte';
import SearchView from './SearchView.svelte';
import SourceView from './SourceView.svelte';

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
const options = () => [...document.querySelectorAll('[role="option"]')] as HTMLElement[];
const button = (name: RegExp) => [...document.querySelectorAll('button')].find((b) => name.test(b.textContent ?? '')) as HTMLButtonElement;
const gapRow = (id: number, shortName: string): api.GapRow => ({ id, kind: 'requirement', name: 'n', shortName, qualifiedName: `R::${shortName}`, file: 'r.sysml', line: id });
const hitRow = (over: Partial<api.SearchHit> = {}): api.SearchHit => ({ id: 1, kind: 'requirement', qualifiedName: 'R::a', shortName: 'X-1', file: 'r.sysml', line: 2, column: 1, ...over });

describe('CoverageView', () => {
  const props = (over: object = {}) => ({ filter: 'no-parent' as api.GapFilter, counts: { 'no-parent': 2, untraced: 7 }, onfilter: vi.fn(), onselect: vi.fn(), ...over });

  // @verifies CT-TRC-004
  it('lists the elements of the chosen filter and selects one on click', async () => {
    vi.mocked(api.coverageGaps).mockResolvedValue({ offset: 0, total: 2, more: false, rows: [gapRow(4, 'X-001'), gapRow(9, 'X-004')] });
    const p = props();
    instance = mount(CoverageView, { target: document.body, props: p });
    await settle();
    expect(api.coverageGaps).toHaveBeenCalledWith('no-parent', 0, api.MAX_PAGE);
    expect(options().map((o) => o.textContent?.trim())).toEqual(['X-001 R::X-001', 'X-004 R::X-004']);
    options()[1].click();
    expect(p.onselect).toHaveBeenCalledWith(9);
  });

  // @verifies CT-TRC-004
  it('has a button per filter with its count, and asks to change filter', async () => {
    vi.mocked(api.coverageGaps).mockResolvedValue({ offset: 0, total: 0, more: false, rows: [] });
    const p = props();
    instance = mount(CoverageView, { target: document.body, props: p });
    await settle();
    expect(button(/^No parent/).textContent).toBe('No parent (2)');
    expect(button(/^No parent/).getAttribute('aria-pressed')).toBe('true');
    button(/^Untraced/).click();
    expect(p.onfilter).toHaveBeenCalledWith('untraced');
  });

  // @verifies CT-TRC-004
  it('says so when a filter lists nothing', async () => {
    vi.mocked(api.coverageGaps).mockResolvedValue({ offset: 0, total: 0, more: false, rows: [] });
    instance = mount(CoverageView, { target: document.body, props: props() });
    await settle();
    expect(document.body.textContent).toContain('Nothing to show');
  });

  // @verifies CT-EFF-005, CT-TRC-004
  it('marks the view with its filter once the first page is in, for the in-app probe', async () => {
    vi.mocked(api.coverageGaps).mockResolvedValue({ offset: 0, total: 1, more: false, rows: [gapRow(4, 'X-001')] });
    instance = mount(CoverageView, { target: document.body, props: props({ filter: 'untraced' }) });
    expect(document.querySelector('[data-view="coverage"]')?.hasAttribute('data-ready')).toBe(false);
    await settle();
    const view = document.querySelector('[data-view="coverage"]') as HTMLElement;
    expect(view.dataset.filter).toBe('untraced');
    expect(view.hasAttribute('data-ready')).toBe(true);
  });

  // @verifies CT-USE-007
  it('says what went wrong when the list cannot load', async () => {
    vi.mocked(api.coverageGaps).mockRejectedValue(new api.ClewError({ what: 'no project is open', where: '', help: 'Open a project folder' }));
    instance = mount(CoverageView, { target: document.body, props: props() });
    await settle();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('no project is open');
  });
});

describe('SearchView', () => {
  const props = (over: object = {}) => ({ query: '', onquery: vi.fn(), onselect: vi.fn(), onopen: vi.fn(), ...over });
  const input = () => document.querySelector('#search-input') as HTMLInputElement;

  // @verifies CT-EFF-006
  it('searches as the user types and shows the hits', async () => {
    vi.mocked(api.search).mockResolvedValue({ offset: 0, total: null, more: false, rows: [hitRow(), hitRow({ id: 2, shortName: 'X-2', qualifiedName: 'R::b' })] });
    const p = props();
    instance = mount(SearchView, { target: document.body, props: p });
    input().value = 'budget';
    input().dispatchEvent(new Event('input', { bubbles: true }));
    expect(p.onquery).toHaveBeenCalledWith('budget');
  });

  // @verifies CT-EFF-006
  it('shows the hits for a query it is given, without waiting for more than the answer', async () => {
    vi.mocked(api.search).mockResolvedValue({ offset: 0, total: null, more: false, rows: [hitRow(), hitRow({ id: 2, shortName: 'X-2', qualifiedName: 'R::b' })] });
    instance = mount(SearchView, { target: document.body, props: props({ query: 'budget' }) });
    await settle();
    expect(api.search).toHaveBeenCalledWith('budget', 0, api.MAX_PAGE);
    expect(options().map((o) => o.textContent?.trim())).toEqual(['requirement X-1 R::a', 'requirement X-2 R::b']);
    const view = document.querySelector('[data-view="search"]') as HTMLElement;
    expect(view.dataset.query).toBe('budget');
    expect(view.hasAttribute('data-ready')).toBe(true);
  });

  // @verifies CT-EFF-006
  it('does not search for a blank query', async () => {
    instance = mount(SearchView, { target: document.body, props: props({ query: '   ' }) });
    await settle();
    expect(api.search).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('Type to search');
  });

  // @verifies CT-EFF-006, CT-TRC-004
  it('selects the element of a hit and opens the file of a hit with no element', async () => {
    vi.mocked(api.search).mockResolvedValue({ offset: 0, total: null, more: false, rows: [hitRow(), hitRow({ id: null, file: 'bad.sysml', line: 8 })] });
    const p = props({ query: 'x' });
    instance = mount(SearchView, { target: document.body, props: p });
    await settle();
    options()[0].click();
    expect(p.onselect).toHaveBeenCalledWith(1);
    options()[1].click();
    expect(p.onopen).toHaveBeenCalledWith('bad.sysml', 8);
  });

  // @verifies CT-USE-007
  it('says what went wrong when the index cannot be searched', async () => {
    vi.mocked(api.search).mockRejectedValue(new api.ClewError({ what: 'the search index is unavailable', where: '.clewtrace', help: 'Delete the folder and open the project again' }));
    instance = mount(SearchView, { target: document.body, props: props({ query: 'x' }) });
    await settle();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('search index is unavailable');
  });
});

describe('SourceView', () => {
  // @verifies CT-TRC-004
  it('shows the file with line numbers and selects the tagged line', async () => {
    vi.mocked(api.sourceLines).mockResolvedValue({
      offset: 0,
      total: 3,
      more: false,
      rows: [
        { line: 1, text: 'a' },
        { line: 2, text: '// @implements X-1' },
        { line: 3, text: 'fn f() {}' },
      ],
    });
    instance = mount(SourceView, { target: document.body, props: { path: 'src/a.rs', line: 2 } });
    await settle();
    expect(api.sourceLines).toHaveBeenCalledWith('src/a.rs', 0, api.MAX_PAGE);
    expect(document.body.textContent).toContain('src/a.rs:2');
    expect(options().map((o) => o.textContent?.trim())).toEqual(['1  a', '2  // @implements X-1', '3  fn f() {}']);
    expect(options()[1].getAttribute('aria-selected')).toBe('true');
  });

  // @verifies CT-USE-007, CT-SEC-003
  it('says what went wrong when the file cannot be read', async () => {
    vi.mocked(api.sourceLines).mockRejectedValue(new api.ClewError({ what: 'that path is not inside the project', where: '../x', help: 'Open a file by a path relative to the project folder.' }));
    instance = mount(SourceView, { target: document.body, props: { path: '../x', line: 1 } });
    await settle();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('not inside the project');
  });
});
// @end
