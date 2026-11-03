// @begin CT-USE-010
// Tests for the shared tooltip: shown on hover and keyboard focus, closed with Esc, linked
// for screen readers, and giving the control's name and shortcut.
import { afterEach, describe, expect, it } from 'vitest';
import { tip, tipText } from './tooltip';

function control(spec: Parameters<typeof tip>[1]): { node: HTMLButtonElement; stop: () => void } {
  const node = document.createElement('button');
  node.textContent = 'Save';
  document.body.append(node);
  const action = tip(node, spec);
  return { node, stop: () => action.destroy() };
}

const shown = () => document.querySelector<HTMLElement>('[role="tooltip"]');

afterEach(() => {
  document.body.innerHTML = '';
});

describe('tipText', () => {
  // @verifies CT-USE-010
  it('gives the name, then the shortcut in brackets when there is one', () => {
    expect(tipText({ name: 'Save' })).toBe('Save');
    expect(tipText({ name: 'Save', shortcut: 'Ctrl+S' })).toBe('Save (Ctrl+S)');
  });
});

describe('tooltip', () => {
  // @verifies CT-USE-010
  it('shows the name and shortcut on hover and hides on leave', () => {
    const { node } = control({ name: 'Save', shortcut: 'Ctrl+S' });
    expect(shown()?.hidden ?? true).toBe(true);
    node.dispatchEvent(new MouseEvent('mouseenter'));
    expect(shown()?.hidden).toBe(false);
    expect(shown()?.textContent).toBe('Save (Ctrl+S)');
    node.dispatchEvent(new MouseEvent('mouseleave'));
    expect(shown()?.hidden).toBe(true);
  });

  // @verifies CT-USE-010
  it('shows on keyboard focus and hides on blur', () => {
    const { node } = control('Undo');
    node.dispatchEvent(new FocusEvent('focus'));
    expect(shown()?.textContent).toBe('Undo');
    node.dispatchEvent(new FocusEvent('blur'));
    expect(shown()?.hidden).toBe(true);
  });

  // @verifies CT-USE-010
  it('links the control to the tooltip while it is shown, for screen readers', () => {
    const { node } = control({ name: 'Redo' });
    node.dispatchEvent(new FocusEvent('focus'));
    expect(node.getAttribute('aria-describedby')).toBe(shown()?.id);
    expect(shown()?.getAttribute('role')).toBe('tooltip');
    node.dispatchEvent(new FocusEvent('blur'));
    expect(node.hasAttribute('aria-describedby')).toBe(false);
  });

  // @verifies CT-USE-010
  it('closes with Esc and leaves other keys alone', () => {
    const { node } = control({ name: 'Save' });
    node.dispatchEvent(new FocusEvent('focus'));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'a' }));
    expect(shown()?.hidden).toBe(false);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(shown()?.hidden).toBe(true);
    expect(node.hasAttribute('aria-describedby')).toBe(false);
  });

  // @verifies CT-USE-010
  it('uses one shared element, follows spec updates, and never sets a title', () => {
    const a = control({ name: 'One' });
    const b = control({ name: 'Two' });
    a.node.dispatchEvent(new FocusEvent('focus'));
    b.node.dispatchEvent(new FocusEvent('focus'));
    expect(document.querySelectorAll('[role="tooltip"]')).toHaveLength(1);
    expect(shown()?.textContent).toBe('Two');
    expect(a.node.hasAttribute('aria-describedby')).toBe(false);
    expect(a.node.hasAttribute('title')).toBe(false);
  });

  // @verifies CT-USE-010
  it('removes its listeners and link when the control goes away', () => {
    const { node, stop } = control({ name: 'Save' });
    node.dispatchEvent(new FocusEvent('focus'));
    stop();
    expect(shown()?.hidden).toBe(true);
    node.dispatchEvent(new FocusEvent('focus'));
    expect(shown()?.hidden).toBe(true);
  });
});
// @end

// @begin CT-USE-010
// No component may use a title attribute: the shared tooltip replaces it.
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

function svelteFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? svelteFiles(join(dir, e.name)) : e.name.endsWith('.svelte') ? [join(dir, e.name)] : [],
  );
}

describe('no title attributes', () => {
  // @verifies CT-USE-010
  it('appears in no component', () => {
    const bad = svelteFiles(__dirname + '/..').filter((f) => /<(?!TraceList)[a-z][^>]*\stitle=/.test(readFileSync(f, 'utf8')));
    expect(bad).toEqual([]);
  });
});
// @end
