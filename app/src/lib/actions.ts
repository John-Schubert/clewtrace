// The action registry: every user action registers here once, and the command palette lists
// them. Later tasks add their actions with `registry.register`.

// @implements CT-USE-002
export interface Action {
  /** Stable dotted ID, such as `project.open`. */
  id: string;
  /** What the palette shows and searches. */
  title: string;
  /** Optional key hint shown beside the title, such as `Ctrl+Shift+P`. */
  shortcut?: string;
  run: () => void | Promise<void>;
}

// @implements CT-USE-002
export class ActionRegistry {
  private readonly actions = new Map<string, Action>();

  /** Adds an action and returns a function that removes it. */
  register(action: Action): () => void {
    if (this.actions.has(action.id)) {
      throw new Error(`Action ${action.id} is already registered. Use a unique ID.`);
    }
    this.actions.set(action.id, action);
    return () => {
      this.actions.delete(action.id);
    };
  }

  list(): Action[] {
    return [...this.actions.values()];
  }

  async run(id: string): Promise<void> {
    const action = this.actions.get(id);
    if (!action) {
      throw new Error(`Unknown action ${id}. Open the command palette to see the actions.`);
    }
    await action.run();
  }
}

// @implements CT-USE-002
/** Actions whose title contains every word of the query, in any order; best match first. */
export function filterActions(actions: Action[], query: string): Action[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) {
    return actions;
  }
  const scored: { action: Action; score: number }[] = [];
  for (const action of actions) {
    const title = action.title.toLowerCase();
    let score = 0;
    let all = true;
    for (const word of words) {
      const at = title.indexOf(word);
      if (at < 0) {
        all = false;
        break;
      }
      // Earlier and word-start matches rank higher.
      score += at + (at === 0 || title[at - 1] === ' ' ? 0 : 50);
    }
    if (all) {
      scored.push({ action, score });
    }
  }
  return scored.sort((a, b) => a.score - b.score).map((s) => s.action);
}
