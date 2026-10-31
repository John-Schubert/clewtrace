// @begin CT-EFF-005, CT-USE-002
// Tests for the shared components: they render in happy-dom and respond to the keyboard.
import { flushSync, mount, unmount } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import CommandPalette from './CommandPalette.svelte';
import VirtualListHarness from './VirtualListHarness.test.svelte';

let instance: Record<string, unknown> | undefined;
afterEach(() => {
  if (instance) {
    unmount(instance);
    instance = undefined;
  }
  document.body.innerHTML = '';
});

describe('VirtualList', () => {
  // @verifies CT-EFF-005
  it('puts only a window of a 100,000 row list in the page', () => {
    instance = mount(VirtualListHarness, { target: document.body, props: { count: 100_000 } });
    flushSync();
    const rows = document.querySelectorAll('[role="option"]');
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThan(60);
    expect(rows[0].getAttribute('aria-setsize')).toBe('100000');
  });

  // @verifies CT-EFF-005
  it('moves the selection with the arrow keys', () => {
    instance = mount(VirtualListHarness, { target: document.body, props: { count: 100_000 } });
    flushSync();
    const list = document.querySelector('[role="listbox"]') as HTMLElement;
    list.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    list.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    flushSync();
    expect(document.querySelector('[aria-selected="true"]')?.getAttribute('aria-posinset')).toBe('2');
  });
});

describe('CommandPalette', () => {
  const actions = [
    { id: 'a.one', title: 'Open project folder', run: () => {} },
    { id: 'a.two', title: 'Theme: dark', run: () => {} },
  ];

  // @verifies CT-USE-002
  it('filters as the user types and runs the chosen action on Enter', () => {
    const onrun = vi.fn();
    instance = mount(CommandPalette, { target: document.body, props: { actions, onrun, onclose: () => {} } });
    flushSync();
    const input = document.querySelector('input') as HTMLInputElement;
    expect(document.activeElement).toBe(input);
    input.value = 'dark';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    expect(document.querySelectorAll('li[role="option"]').length).toBe(1);
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    expect(onrun).toHaveBeenCalledWith('a.two');
  });

  // @verifies CT-USE-002
  it('closes on Escape', () => {
    const onclose = vi.fn();
    instance = mount(CommandPalette, { target: document.body, props: { actions, onrun: () => {}, onclose } });
    flushSync();
    document.querySelector('input')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(onclose).toHaveBeenCalledOnce();
  });
});
// @end
