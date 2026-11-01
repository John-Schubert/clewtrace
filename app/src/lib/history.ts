// Back and forward through the elements the user has looked at. Holds element IDs only.

// @begin CT-TRC-005, CT-EFF-004
/** The most elements the history keeps; the oldest are dropped first. */
const LIMIT = 100;
// @end

// @implements CT-TRC-005
export class NavHistory {
  private trail: number[] = [];
  private at = -1;

  get current(): number | undefined {
    return this.trail[this.at];
  }

  get canBack(): boolean {
    return this.at > 0;
  }

  get canForward(): boolean {
    return this.at >= 0 && this.at < this.trail.length - 1;
  }

  /** Records a new element, dropping any forward trail. Visiting the current one does nothing. */
  visit(id: number): void {
    if (id === this.current) {
      return;
    }
    this.trail = [...this.trail.slice(0, this.at + 1), id].slice(-LIMIT);
    this.at = this.trail.length - 1;
  }

  /** Steps back and returns that element, or undefined at the start. */
  back(): number | undefined {
    return this.canBack ? this.trail[--this.at] : undefined;
  }

  /** Steps forward and returns that element, or undefined at the end. */
  forward(): number | undefined {
    return this.canForward ? this.trail[++this.at] : undefined;
  }

  /**
   * Steps back or forward to the nearest element that still exists, going past those that
   * `exists` says are gone (an edit removed or renamed them). Returns that element, or
   * undefined, and stays where it is, when none is left in that direction.
   */
  async step(direction: 'back' | 'forward', exists: (id: number) => Promise<boolean>): Promise<number | undefined> {
    const by = direction === 'back' ? -1 : 1;
    for (let at = this.at + by; at >= 0 && at < this.trail.length; at += by) {
      if (await exists(this.trail[at])) {
        this.at = at;
        return this.trail[at];
      }
    }
    return undefined;
  }

  clear(): void {
    this.trail = [];
    this.at = -1;
  }
}
