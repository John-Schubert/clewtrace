// @begin CT-EFF-008, CT-USE-009
// File watching in the UI. The Rust side watches the project folder and re-reads what
// changed; the UI waits for its news with one long call at a time, never a poll, and
// decides what the open file's editor does. The logic lives here so components stay thin.
import type { DiskNews } from './api';
// @end

// @implements CT-USE-009, CT-USE-008
/**
 * What the editor does after file events: nothing when its file did not change, reload a
 * clean editor from disk, or ask when it has unsaved edits (a git pull, another editor).
 */
export function decideDisk(open: string | undefined, dirty: boolean, news: DiskNews): 'none' | 'reload' | 'ask' {
  if (open === undefined || (!news.reloaded.includes(open) && !news.conflicts.includes(open))) {
    return 'none';
  }
  return dirty ? 'ask' : 'reload';
}

// @implements CT-EFF-008, CT-USE-009
/**
 * Waits for file event news, hands each reply to `heard`, and waits again. It stops when
 * the reply says the project is not watched, and after a failed wait, which `failed`
 * hears; `start` begins again when a project opens. One wait runs at a time, and between
 * changes the call simply stays open, so nothing runs while no files change.
 */
export class DiskWatch {
  private loop: Promise<void> | undefined;
  /** `start` was called while a wait ran, so a reply that ends watching is not the end. */
  private again = false;

  constructor(
    private readonly wait: () => Promise<DiskNews>,
    private readonly heard: (news: DiskNews) => void | Promise<void>,
    private readonly failed: (e: unknown) => void = () => {},
  ) {}

  /**
   * Starts waiting, unless a wait is already running, in which case that loop waits at
   * least once more: its reply may be the last project's. Resolves when watching stops.
   */
  start(): Promise<void> {
    if (this.loop) {
      this.again = true;
      return this.loop;
    }
    this.loop = this.run().finally(() => (this.loop = undefined));
    return this.loop;
  }

  get running(): boolean {
    return this.loop !== undefined;
  }

  private async run(): Promise<void> {
    for (;;) {
      let news: DiskNews;
      try {
        news = await this.wait();
      } catch (e) {
        this.failed(e);
        return;
      }
      const empty = !news.reloaded.length && !news.conflicts.length && !news.problems.length && !news.summary;
      if (!empty) {
        await this.heard(news);
      }
      if (!news.watching) {
        if (!this.again) {
          return;
        }
        this.again = false;
      }
    }
  }
}
