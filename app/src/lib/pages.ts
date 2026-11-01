// A list backed by pages fetched on demand. The UI holds only the pages it has asked for.
import { MAX_PAGE, type Page } from './api';

// @implements CT-EFF-005, CT-USE-007
export class PagedRows<Row> {
  count = 0;
  private readonly loaded = new Map<number, Row[]>();
  private readonly pending = new Map<number, Promise<void>>();
  private generation = 0;

  constructor(
    private readonly fetch: (offset: number, limit: number) => Promise<Page<Row>>,
    private readonly onchange: () => void,
    private readonly pageSize = MAX_PAGE,
    private readonly onerror: (e: unknown) => void = () => {},
  ) {}

  /** Forgets every page and loads the first, which tells how many rows there are. */
  async reset(): Promise<void> {
    this.generation++;
    this.loaded.clear();
    this.pending.clear();
    this.count = 0;
    await this.load(0);
    this.onchange();
  }

  /**
   * Fetches again every page it holds, after the data changed, and swaps them in together,
   * so the list is never shown empty. When a fetch fails, the old rows stay.
   */
  async refresh(): Promise<void> {
    const generation = ++this.generation;
    this.pending.clear();
    const held = [...new Set([0, ...this.loaded.keys()])].sort((a, b) => a - b);
    try {
      const results = await Promise.all(held.map((page) => this.fetch(page * this.pageSize, this.pageSize)));
      if (generation !== this.generation) {
        return;
      }
      this.loaded.clear();
      results.forEach((result, i) => this.loaded.set(held[i], result.rows));
      const last = results[results.length - 1];
      const known = held[held.length - 1] * this.pageSize + last.rows.length;
      this.count = results[0].total ?? (last.more ? known + 1 : known);
      this.onchange();
    } catch (e) {
      this.onerror(e);
    }
  }

  /** The row at an index, or undefined while its page loads (which this starts). */
  at(index: number): Row | undefined {
    const page = Math.floor(index / this.pageSize);
    const rows = this.loaded.get(page);
    if (rows === undefined) {
      void this.load(page);
      return undefined;
    }
    return rows[index - page * this.pageSize];
  }

  /** Resolves when every page asked for so far has arrived. */
  async idle(): Promise<void> {
    while (this.pending.size > 0) {
      await Promise.all([...this.pending.values()]);
    }
  }

  private load(page: number): Promise<void> {
    const running = this.pending.get(page);
    if (running) {
      return running;
    }
    const generation = this.generation;
    const task = this.fetch(page * this.pageSize, this.pageSize)
      .then((result) => {
        if (generation !== this.generation) {
          return;
        }
        this.loaded.set(page, result.rows);
        // With no total (search), the count grows a page at a time: one placeholder row stands
        // for the next page, and loads it when it is scrolled into view.
        const known = page * this.pageSize + result.rows.length;
        this.count = result.total ?? (result.more ? Math.max(this.count, known + 1) : known);
        this.onchange();
      })
      .catch((e: unknown) => this.onerror(e))
      .finally(() => {
        if (this.pending.get(page) === task) {
          this.pending.delete(page);
        }
      });
    this.pending.set(page, task);
    return task;
  }
}
