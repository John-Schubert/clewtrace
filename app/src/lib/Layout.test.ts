import { mount, tick, unmount } from 'svelte';
import { describe, expect, it } from 'vitest';
import LayoutHarness from './LayoutHarness.test.svelte';

// @verifies CT-USE-004
describe('Layout', () => {
  it('has one landmark for each pane, in focus order', async () => {
    const target = document.createElement('div');
    document.body.append(target);
    const app = mount(LayoutHarness, { target });
    await tick();
    const order = [...target.querySelectorAll('header, nav, main, aside, section[aria-label]')].map(
      (el) => el.getAttribute('aria-label') ?? el.tagName.toLowerCase(),
    );
    expect(order).toEqual(['header', 'Model browser', 'main', 'Trace panel', 'Problems']);
    expect(target.querySelectorAll('main')).toHaveLength(1);
    expect(target.querySelector('a.skip')?.getAttribute('href')).toBe('#main');
    expect(target.querySelector('main')?.id).toBe('main');
    unmount(app);
    target.remove();
  });
});
