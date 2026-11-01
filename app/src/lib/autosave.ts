// @begin CT-USE-008, CT-EFF-008
// Autosave for the text editor: after a short pause in typing, when the editor loses focus,
// and before the window closes. Every save goes through the Rust side's atomic write. The
// logic lives here so the editor component stays thin.
import { QuietTimer } from './live';

/** How long the text must be still before it is saved, in milliseconds. */
export const AUTOSAVE_MS = 1000;
// @end

// @implements CT-USE-008, CT-EFF-008
/**
 * Saves the editor's text when it has been still for a short pause, or at once on `flush`.
 * One save runs at a time; edits made during a save are saved after it. The pause is a
 * one-shot timer, never a poll, so nothing runs while the text does not change.
 */
export class Autosaver {
  private readonly quiet: QuietTimer;
  private running: Promise<boolean> | undefined;

  /**
   * `save` writes the current text and resolves true when it was written. `dirty` says
   * whether the text differs from what is on disk.
   */
  constructor(
    private readonly save: () => Promise<boolean>,
    private readonly dirty: () => boolean,
    ms = AUTOSAVE_MS,
  ) {
    this.quiet = new QuietTimer(ms);
  }

  /** The text changed: save it after the pause, unless it changes again first. */
  changed(): void {
    this.quiet.schedule(() => void this.flush());
  }

  /**
   * Saves now. Waits for a save that is already running, then saves again if the text
   * still differs. Resolves true when nothing is left unsaved, false when a save failed.
   */
  async flush(): Promise<boolean> {
    this.quiet.cancel();
    // A save that succeeds always ends the edits it saw, so this ends once typing stops.
    for (;;) {
      while (this.running) {
        await this.running;
      }
      if (!this.dirty()) {
        return true;
      }
      const run = this.save().catch(() => false);
      this.running = run;
      const ok = await run;
      if (this.running === run) {
        this.running = undefined;
      }
      if (!ok) {
        return false;
      }
    }
  }

  /** Drops a save that is waiting for the pause to end. A running save is not stopped. */
  cancel(): void {
    this.quiet.cancel();
  }

  /** Whether a save is waiting for the pause to end. */
  get pending(): boolean {
    return this.quiet.pending;
  }
}

// @implements CT-USE-008
/** The part of a Tauri window that `saveOnClose` uses, so tests can pass a fake. */
export interface CloseRequests {
  onCloseRequested(handler: (event: { preventDefault(): void }) => Promise<void>): Promise<() => void>;
}

// @implements CT-USE-008, CT-USE-007
/**
 * Saves the edits before the window closes. When the save fails, the window stays open
 * once and `failed` tells the user, whose edits are still in the editor; closing again
 * closes it. Resolves to the function that stops listening.
 */
export function saveOnClose(win: CloseRequests, flush: () => Promise<boolean>, failed: () => void): Promise<() => void> {
  let warned = false;
  return win.onCloseRequested(async (event) => {
    if ((await flush()) || warned) {
      return;
    }
    warned = true;
    event.preventDefault();
    failed();
  });
}
