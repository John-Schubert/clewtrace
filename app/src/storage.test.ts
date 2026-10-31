// @begin CT-SEC-003
// Guard: the UI keeps no project content in browser storage. It asks the Rust core each time.
// Only the files in ALLOWED may use these APIs, and only for UI preferences.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';

const ROOT = join(import.meta.dirname);
const FORBIDDEN = /\b(localStorage|sessionStorage|indexedDB|IDBDatabase|IDBFactory)\b/;
// Each entry gives its reason. A new entry needs John's approval.
const ALLOWED: Record<string, string> = {
  'lib/theme.ts': 'stores the theme choice (light, dark, or system), a UI preference',
  'lib/theme.test.ts': 'tests the theme preference',
  'storage.test.ts': 'this guard',
};

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? sources(path) : /\.(ts|svelte)$/.test(name) ? [path] : [];
  });
}

describe('browser storage', () => {
  // @verifies CT-SEC-003
  it('is not used outside UI preferences', () => {
    const offenders = sources(ROOT)
      .map((p) => relative(ROOT, p).split('\\').join('/'))
      .filter((rel) => !(rel in ALLOWED))
      .filter((rel) => FORBIDDEN.test(readFileSync(join(ROOT, rel), 'utf8')));
    expect(offenders).toEqual([]);
  });

  // @verifies CT-SEC-003
  it('has no stale allowlist entries', () => {
    const present = new Set(sources(ROOT).map((p) => relative(ROOT, p).split('\\').join('/')));
    expect(Object.keys(ALLOWED).filter((rel) => !present.has(rel))).toEqual([]);
  });
});
// @end
