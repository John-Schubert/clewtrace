import { describe, expect, it } from 'vitest';

// @begin CT-SEC-003
// Files that may use browser storage, each with its reason. The UI never keeps project
// content in storage; it asks the Rust core each time (CLAUDE.md, CT-SEC-003).
const ALLOWED: Record<string, string> = {
  './theme.ts': 'UI preference: the theme choice only, never project content',
};

const sources = import.meta.glob(['../**/*.ts', '../**/*.svelte', '!../**/*.test.ts'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const STORAGE = /\b(localStorage|sessionStorage|indexedDB|IDBFactory|openDatabase)\b/;
// @end

// @verifies CT-SEC-003
describe('browser storage', () => {
  it('finds the UI sources it is meant to check', () => {
    expect(Object.keys(sources).length).toBeGreaterThan(3);
  });

  it('is not used outside the allowlist', () => {
    const offenders = Object.entries(sources)
      .filter(([path]) => !Object.keys(ALLOWED).some((a) => path.endsWith(a.slice(1))))
      .filter(([, text]) => STORAGE.test(text))
      .map(([path]) => path);
    expect(offenders).toEqual([]);
  });

  it('gives every allowlist entry a reason and a real file', () => {
    for (const [file, reason] of Object.entries(ALLOWED)) {
      expect(reason.length).toBeGreaterThan(10);
      expect(Object.keys(sources).some((p) => p.endsWith(file.slice(1)))).toBe(true);
    }
  });
});
