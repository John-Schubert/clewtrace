// The model tree as a flat list of slots. Expanding a node inserts a slot for each of its
// children; a slot gets its row when the page holding it arrives, so a package with
// thousands of members costs one page until the user scrolls further.
import { MAX_PAGE, type Page, type TreeRow } from './api';

// @implements CT-MOD-005, CT-EFF-005
/** One visible line of the tree. `row` is undefined while its page loads. */
export interface TreeNode {
  depth: number;
  /** The element ID of the parent (0 for the root) and the position among its children. */
  parent: number;
  offset: number;
  row?: TreeRow;
  expanded: boolean;
}

// @implements CT-MOD-005
export type FetchChildren = (parent: number, offset: number, limit: number) => Promise<Page<TreeRow>>;

// @implements CT-MOD-005, CT-EFF-005, CT-USE-007
export class TreeModel {
  private flat: TreeNode[] = [];
  /** The child slots of each expanded parent, by element ID. */
  private readonly kids = new Map<number, TreeNode[]>();
  private readonly pending = new Map<string, Promise<void>>();
  private generation = 0;

  constructor(
    private readonly fetch: FetchChildren,
    private readonly onchange: () => void,
    private readonly pageSize = MAX_PAGE,
    private readonly onerror: (e: unknown) => void = () => {},
  ) {}

  get count(): number {
    return this.flat.length;
  }

  /** Forgets the tree and loads the first page of the root. */
  async reset(): Promise<void> {
    this.generation++;
    this.flat = [];
    this.kids.clear();
    this.pending.clear();
    const first = await this.fetchPage(0, 0);
    if (first) {
      this.flat = this.slots(0, first.total ?? first.rows.length, 0);
      this.fill(0, first);
    }
    this.onchange();
  }

  /**
   * Fetches the tree again after the model changed, keeping every package that was open
   * open. The new tree is built off to the side and swapped in whole, so the list is never
   * shown empty or half built. A package whose ID is gone closes. Rows past the first page
   * of a package load as they scroll into view; a package open there is not reopened.
   * When a fetch fails, the old tree stays.
   */
  async refresh(): Promise<void> {
    const generation = ++this.generation;
    const open = new Set<number>();
    for (const node of this.flat) {
      if (node.expanded && node.row) {
        open.add(node.row.id);
      }
    }
    const kids = new Map<number, TreeNode[]>();
    const build = async (parent: number, depth: number): Promise<TreeNode[] | undefined> => {
      const first = await this.fetchPage(parent, 0);
      if (!first || generation !== this.generation) {
        return undefined;
      }
      const slots = Array.from({ length: first.total ?? first.rows.length }, (_, offset) => ({
        depth,
        parent,
        offset,
        expanded: false,
      })) as TreeNode[];
      kids.set(parent, slots);
      first.rows.forEach((row, i) => {
        if (slots[first.offset + i]) {
          slots[first.offset + i].row = row;
        }
      });
      const flat: TreeNode[] = [];
      for (const slot of slots) {
        flat.push(slot);
        if (slot.row && slot.row.children > 0 && open.has(slot.row.id)) {
          const below = await build(slot.row.id, depth + 1);
          if (below === undefined) {
            return undefined;
          }
          slot.expanded = true;
          flat.push(...below);
        }
      }
      return flat;
    };
    const flat = await build(0, 0);
    if (flat !== undefined && generation === this.generation) {
      this.flat = flat;
      this.kids.clear();
      kids.forEach((v, k) => this.kids.set(k, v));
      this.pending.clear();
      this.onchange();
    }
  }

  /** The node at an index. A node without a row starts the load of its page. */
  at(index: number): TreeNode | undefined {
    const node = this.flat[index];
    if (node && node.row === undefined) {
      void this.loadPage(node.parent, Math.floor(node.offset / this.pageSize));
    }
    return node;
  }

  /** The index of the visible row for an element, or -1 when it is not shown. */
  indexOf(id: number): number {
    return this.flat.findIndex((n) => n.row?.id === id);
  }

  /** Resolves when every page asked for so far has arrived. */
  async idle(): Promise<void> {
    while (this.pending.size > 0) {
      await Promise.all([...this.pending.values()]);
    }
  }

  async toggle(index: number): Promise<void> {
    if (this.flat[index]?.expanded) {
      this.collapse(index);
    } else {
      await this.expand(index);
    }
  }

  async expand(index: number): Promise<void> {
    const node = this.flat[index];
    if (!node?.row || node.expanded || node.row.children === 0) {
      return;
    }
    const slots = this.slots(node.row.id, node.row.children, node.depth + 1);
    this.flat.splice(index + 1, 0, ...slots);
    node.expanded = true;
    this.onchange();
    await this.loadPage(node.row.id, 0);
  }

  collapse(index: number): void {
    const node = this.flat[index];
    if (!node?.expanded) {
      return;
    }
    let end = index + 1;
    while (end < this.flat.length && this.flat[end].depth > node.depth) {
      end++;
    }
    for (const gone of this.flat.splice(index + 1, end - index - 1)) {
      if (gone.row) {
        this.kids.delete(gone.row.id);
      }
    }
    this.kids.delete(node.row?.id ?? -1);
    node.expanded = false;
    this.onchange();
  }

  private slots(parent: number, count: number, depth: number): TreeNode[] {
    const slots = Array.from({ length: count }, (_, offset) => ({ depth, parent, offset, expanded: false }));
    this.kids.set(parent, slots);
    return slots;
  }

  private fill(parent: number, page: Page<TreeRow>): void {
    const slots = this.kids.get(parent);
    page.rows.forEach((row, i) => {
      const slot = slots?.[page.offset + i];
      if (slot) {
        slot.row = row;
      }
    });
  }

  private async fetchPage(parent: number, page: number): Promise<Page<TreeRow> | undefined> {
    try {
      return await this.fetch(parent, page * this.pageSize, this.pageSize);
    } catch (e) {
      this.onerror(e);
      return undefined;
    }
  }

  private loadPage(parent: number, page: number): Promise<void> {
    const key = `${parent}:${page}`;
    const running = this.pending.get(key);
    if (running) {
      return running;
    }
    const generation = this.generation;
    const task = this.fetchPage(parent, page)
      .then((result) => {
        // A page for a parent that was collapsed or a tree that was reset is dropped.
        if (result && generation === this.generation && this.kids.has(parent)) {
          this.fill(parent, result);
          this.onchange();
        }
      })
      .finally(() => this.pending.delete(key));
    this.pending.set(key, task);
    return task;
  }
}
