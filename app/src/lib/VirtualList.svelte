<script lang="ts">
  // @begin CT-EFF-005, CT-USE-004
  // The one list component for any list that can pass 200 rows. It renders only the rows in
  // view, tells the parent which range that is (so the parent can fetch those pages from the
  // Rust core), and works from the keyboard as a listbox.
  import type { Snippet } from 'svelte';
  import { moveIndex, visibleRange } from './virtual';

  let {
    count,
    rowHeight,
    label,
    row,
    onrange = () => {},
    onactivate = () => {},
    active = $bindable(-1),
    height = 400,
  }: {
    count: number;
    rowHeight: number;
    label: string;
    row: Snippet<[number]>;
    onrange?: (start: number, end: number) => void;
    onactivate?: (index: number) => void;
    active?: number;
    height?: number;
  } = $props();

  const OVERSCAN = 5;
  const uid = Math.random().toString(36).slice(2, 8);

  let scrollTop = $state(0);
  let measured = $state(0);
  let box: HTMLDivElement | undefined = $state();

  const viewport = $derived(measured > 0 ? measured : height);
  const range = $derived(visibleRange({ scrollTop, viewport, rowHeight, count, overscan: OVERSCAN }));
  const rows = $derived(Array.from({ length: range.end - range.start }, (_, i) => range.start + i));

  $effect(() => {
    onrange(range.start, range.end);
  });

  function keydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && active >= 0) {
      e.preventDefault();
      onactivate(active);
      return;
    }
    const page = Math.max(1, Math.floor(viewport / rowHeight) - 1);
    const next = moveIndex(active, e.key, count, page);
    if (next === active) {
      return;
    }
    e.preventDefault();
    active = next;
    if (box && next >= 0) {
      const top = next * rowHeight;
      if (top < box.scrollTop) {
        box.scrollTop = top;
      } else if (top + rowHeight > box.scrollTop + viewport) {
        box.scrollTop = top + rowHeight - viewport;
      }
    }
  }
  // @end
</script>

<!-- @begin CT-EFF-005, CT-USE-004 -->
<div
  bind:this={box}
  bind:clientHeight={measured}
  class="list"
  style:height="{height}px"
  role="listbox"
  tabindex="0"
  aria-label={label}
  aria-activedescendant={active >= 0 ? `${uid}-${active}` : undefined}
  onscroll={() => (scrollTop = box?.scrollTop ?? 0)}
  onkeydown={keydown}
>
  <div class="space" style:height="{range.total}px">
    <div class="window" style:transform="translateY({range.offset}px)">
      {#each rows as index (index)}
        <!-- The list box owns the keyboard (aria-activedescendant), so rows need no key handler. -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          id="{uid}-{index}"
          class="row"
          role="option"
          aria-selected={index === active}
          aria-posinset={index + 1}
          aria-setsize={count}
          tabindex="-1"
          style:height="{rowHeight}px"
          onclick={() => {
            active = index;
            onactivate(index);
          }}
        >
          {@render row(index)}
        </div>
      {/each}
    </div>
  </div>
</div>
<!-- @end -->

<style>
  /* @begin CT-EFF-005, CT-USE-004 */
  .list {
    overflow-y: auto;
    position: relative;
  }
  .list:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .space {
    position: relative;
  }
  .window {
    will-change: transform;
  }
  .row {
    box-sizing: border-box;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    cursor: default;
  }
  .row[aria-selected='true'] {
    background: var(--selected-bg);
    color: var(--selected-fg);
  }
  /* @end */
</style>
