// @begin CT-MOD-003, CT-EFF-005, CT-EFF-008, CT-USE-007
// Live validation's logic: when to send the buffer, and how a problem becomes a marker.
// Kept out of the editor component so it can be tested as plain TypeScript.
import type { Diagnostic } from '@codemirror/lint';
import type { Text } from '@codemirror/state';
import { ClewError, type DiagnosticRow } from './api';

/** How long the text must be still before it is sent for checking, in milliseconds. */
export const QUIET_MS = 300;

/**
 * A one-shot timer that each keystroke resets. It runs the check once, after the pause, and
 * leaves no timer behind. It is never a poll (CT-EFF-008).
 */
export class QuietTimer {
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly ms = QUIET_MS) {}

  /** Runs `run` after the pause, replacing any run already waiting. */
  schedule(run: () => void): void {
    this.cancel();
    this.timer = setTimeout(() => {
      this.timer = undefined;
      run();
    }, this.ms);
  }

  cancel(): void {
    if (this.timer !== undefined) {
      clearTimeout(this.timer);
      this.timer = undefined;
    }
  }

  get pending(): boolean {
    return this.timer !== undefined;
  }
}

/**
 * Tells the panes the project was rebuilt from an edit, so they fetch what they show again.
 * It is a plain subscribe and fire: nothing runs between edits.
 */
export class Refresher {
  private readonly listeners = new Set<() => void>();

  /** Calls `listener` on each `fire`. Returns the function that stops it. */
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => void this.listeners.delete(listener);
  }

  fire(): void {
    for (const listener of [...this.listeners]) {
      listener();
    }
  }

  get size(): number {
    return this.listeners.size;
  }
}

/**
 * Whether the element with this ID is still in the model after an edit. Only the Rust
 * side's answer that it is gone counts; a call that failed some other way is thrown, so a
 * broken connection does not clear the user's selection.
 */
export async function stillThere(id: number, detail: (id: number) => Promise<unknown>): Promise<boolean> {
  try {
    await detail(id);
    return true;
  } catch (e) {
    if (e instanceof ClewError) {
      return false;
    }
    throw e;
  }
}

/** The text of a problem and its fix, as the marker's tooltip shows it. */
function message(row: DiagnosticRow): Pick<Diagnostic, 'message' | 'renderMessage'> {
  return {
    message: `${row.what}\nHow to fix: ${row.help}`,
    renderMessage: () => {
      const box = document.createElement('div');
      const what = document.createElement('div');
      what.textContent = row.what;
      const help = document.createElement('div');
      help.textContent = `How to fix: ${row.help}`;
      box.append(what, help);
      return box;
    },
  };
}

/**
 * The editor markers for the problems of the file `doc` holds. A marker covers the word at
 * the problem's column, or one character when it is not on a word. A line outside the text
 * has no marker; the problems list still shows it.
 */
export function markers(doc: Text, rows: DiagnosticRow[]): Diagnostic[] {
  const found: Diagnostic[] = [];
  for (const row of rows) {
    if (!Number.isInteger(row.line) || row.line < 1 || row.line > doc.lines) {
      continue;
    }
    const line = doc.line(row.line);
    const column = Math.min(Math.max(Math.trunc(row.column) || 1, 1), line.length + 1);
    // A problem at the end of the line marks the last character instead.
    const from = Math.min(line.from + column - 1, Math.max(line.from, line.to - 1));
    const rest = /^[A-Za-z0-9_]+/.exec(line.text.slice(from - line.from));
    const to = Math.min(line.to, from + (rest ? rest[0].length : 1));
    found.push({ from, to, severity: row.severity, ...message(row) });
  }
  return found;
}
// @end
