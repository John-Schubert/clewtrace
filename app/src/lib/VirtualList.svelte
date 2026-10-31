<!-- @begin CT-EFF-005, CT-USE-004 -->
<script lang="ts" generics="Row">
  // The one list component for anything that can pass 200 rows. It renders only the rows in
  // view; the caller supplies a row snippet and the number of rows. Arrow keys, Home, and End
  // move the selection; the list is one tab stop (roving selection).
  import type { Snippet } from 'svelte';
  import { scrollTopFor, visibleRange } from './virtual';

  interface Props {
    count: number;
    rowHeight?: number;
    label: string;
    /** The row at an index, or undefined while its page is loading. */
    rowAt: (index: number) => Row | undefined;
    row: Snippet<[Row, number]>;
    selected?: number;
    onselect?: (index: number) => void;
  }
  let { count, rowHeight = 24, label, rowAt, row, selected = $bindable(-1), onselect }: Props = $props();

  let scroller = $state<HTMLDivElement>();
  let viewport = $state(400);
  let scrollTop = $state(0);
  const range = $derived(visibleRange({ count, rowHeight, viewport, scrollTop, overscan: 5 }));
  const indexes = $derived(Array.from({ length: range.end - range.start }, (_, i) => range.start + i));

  function select(index: number) {
    selected = Math.max(0, Math.min(count - 1, index));
    if (scroller) {
      scroller.scrollTop = scrollTopFor({ index: selected, rowHeight, viewport, scrollTop: scroller.scrollTop });
    }
    onselect?.(selected);
  }

  function onkeydown(event: KeyboardEvent) {
    const page = Math.max(1, Math.floor(viewport / rowHeight) - 1);
    const moves: Record<string, number> = {
      ArrowDown: selected + 1,
      ArrowUp: selected - 1,
      PageDown: selected + page,
      PageUp: selected - page,
      Home: 0,
      End: count - 1,
    };
    if (count > 0 && event.key in moves) {
      event.preventDefault();
      select(selected < 0 && event.key === 'ArrowDown' ? 0 : moves[event.key]);
    }
  }
</script>

<div
  class="list"
  role="listbox"
  tabindex="0"
  aria-label={label}
  aria-activedescendant={selected >= 0 ? `${label}-row-${selected}` : undefined}
  bind:this={scroller}
  bind:clientHeight={viewport}
  onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
  {onkeydown}
>
  <div class="space" style:height="{range.totalHeight}px">
    <div class="window" style:transform="translateY({range.offset}px)">
      {#each indexes as i (i)}
        {@const data = rowAt(i)}
        <!-- svelte-ignore a11y_click_events_have_key_events: the listbox handles the keys -->
        <div
          id="{label}-row-{i}"
          class="row"
          class:selected={i === selected}
          role="option"
          tabindex="-1"
          aria-selected={i === selected}
          aria-posinset={i + 1}
          aria-setsize={count}
          style:height="{rowHeight}px"
          onclick={() => select(i)}
        >
          {#if data !== undefined}
            {@render row(data, i)}
          {:else}
            <span class="loading">Loading...</span>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .list {
    overflow-y: auto;
    height: 100%;
    background: var(--surface);
    color: var(--text);
  }
  .list:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .space {
    position: relative;
  }
  .row {
    box-sizing: border-box;
    padding: 0 0.5rem;
    line-height: 1.5;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    cursor: default;
  }
  .row:hover {
    background: var(--row-hover);
  }
  .row.selected {
    background: var(--row-selected);
  }
  .loading {
    color: var(--text-muted);
  }
</style>
<!-- @end -->
