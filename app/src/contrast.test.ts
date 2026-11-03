// @begin CT-USE-004, CT-USE-005
// Tests that the color tokens meet WCAG 2.1 AA in both themes: 4.5:1 for text, 3:1 for the
// focus ring.
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const css = readFileSync(__dirname + '/lib/Tokens.svelte', 'utf8');

function tokens(selector: string): Record<string, string> {
  const block = css.slice(css.indexOf(selector));
  const body = block.slice(block.indexOf('{') + 1, block.indexOf('}'));
  return Object.fromEntries([...body.matchAll(/--([a-z-]+):\s*(#[0-9a-f]{6})/g)].map((m) => [m[1], m[2]]));
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function ratio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const themes = { light: tokens(':global(:root) {'), dark: tokens(":global(:root[data-theme='dark'])") };

describe.each(Object.entries(themes))('%s theme', (_name, t) => {
  // @verifies CT-USE-005
  it('has text at 4.5:1 or more on every background', () => {
    for (const fg of ['text', 'text-muted', 'danger', 'accent']) {
      for (const bg of ['bg', 'surface']) {
        expect(ratio(t[fg], t[bg]), `${fg} on ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    }
    for (const bg of ['row-hover', 'row-selected']) {
      expect(ratio(t.text, t[bg]), `text on ${bg}`).toBeGreaterThanOrEqual(4.5);
      expect(ratio(t['text-muted'], t[bg]), `muted on ${bg}`).toBeGreaterThanOrEqual(4.5);
    }
    expect(ratio(t['accent-text'], t.accent)).toBeGreaterThanOrEqual(4.5);
  });

  // @verifies CT-USE-004
  it('has a focus ring at 3:1 or more', () => {
    expect(ratio(t.focus, t.bg)).toBeGreaterThanOrEqual(3);
    expect(ratio(t.focus, t.surface)).toBeGreaterThanOrEqual(3);
  });
});
// @end
