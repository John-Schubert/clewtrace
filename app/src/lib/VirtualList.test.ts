import { mount, tick, unmount } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import VirtualListHarness from './VirtualListHarness.test.svelte';

// @verifies CT-EFF-005
describe('VirtualList', () => {
  it('renders a window of rows, not all 10,000', async () => {
    const onrange = vi.fn();
    const target = document.createElement('div');
    document.body.append(target);
    const app = mount(VirtualListHarness, { target, props: { count: 10_000, onrange } });
    await tick();
    const rows = target.querySelectorAll('[role="option"]');
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThan(60);
    expect(onrange).toHaveBeenCalledWith(0, expect.any(Number));
    expect(target.querySelector('[role="listbox"]')?.getAttribute('aria-label')).toBe('Test rows');
    expect(rows[0]?.getAttribute('aria-setsize')).toBe('10000');
    unmount(app);
    target.remove();
  });

  it('moves the active row with the arrow keys', async () => {
    const target = document.createElement('div');
    document.body.append(target);
    const app = mount(VirtualListHarness, { target, props: { count: 100, onrange: () => {} } });
    await tick();
    const box = target.querySelector('[role="listbox"]') as HTMLElement;
    box.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    await tick();
    expect(box.getAttribute('aria-activedescendant')).toBe(
      target.querySelector('[aria-selected="true"]')?.id,
    );
    expect(target.querySelector('[aria-selected="true"]')?.textContent).toContain('Row 0');
    unmount(app);
    target.remove();
  });
});
