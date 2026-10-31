// The action registry: every user action joins it, and the command palette lists it.
// Later tasks register their actions here and add nothing to the palette itself.

// @implements CT-USE-002
/** One user action. `keywords` are extra words the palette search matches. */
export interface Action {
  id: string;
  title: string;
  keywords?: string;
  shortcut?: string;
  run: () => void | Promise<void>;
}

// @implements CT-USE-002
/** The registry's operations. */
export interface Registry {
  /** Adds an action; returns a function that removes it. Throws on a duplicate id. */
  register(action: Action): () => void;
  list(): Action[];
  /** Actions whose title or keywords contain every word of `query`, best first. */
  filter(query: string): Action[];
  run(id: string): Promise<void>;
}

// @implements CT-USE-002
export function createRegistry(): Registry {
  const actions = new Map<string, Action>();

  function rank(action: Action, words: string[]): number {
    const title = action.title.toLowerCase();
    const first = words[0] ?? '';
    if (title.startsWith(first)) {
      return 0;
    }
    return title.includes(first) ? 1 : 2;
  }

  return {
    register(action) {
      if (actions.has(action.id)) {
        throw new Error(`Action ${action.id} is already registered`);
      }
      actions.set(action.id, action);
      return () => void actions.delete(action.id);
    },
    list: () => [...actions.values()],
    filter(query) {
      const words = query.toLowerCase().split(/\s+/).filter(Boolean);
      const all = [...actions.values()];
      if (words.length === 0) {
        return all;
      }
      return all
        .filter((a) => {
          const hay = `${a.title} ${a.keywords ?? ''}`.toLowerCase();
          return words.every((w) => hay.includes(w));
        })
        .map((a, i) => ({ a, i, r: rank(a, words) }))
        .sort((x, y) => x.r - y.r || x.i - y.i)
        .map((x) => x.a);
    },
    async run(id) {
      const action = actions.get(id);
      if (!action) {
        throw new Error(`No action ${id}`);
      }
      await action.run();
    },
  };
}

// @begin CT-USE-002
/** The registry of the running app. */
export const registry = createRegistry();
// @end

// @implements CT-USE-002
/** Whether a key event opens the command palette: Ctrl+K or Cmd+K. */
export function isPaletteKey(e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey'>): boolean {
  return e.key.toLowerCase() === 'k' && (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey;
}
