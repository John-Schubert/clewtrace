<!-- @begin CT-TRC-005, CT-TRC-006, CT-MOD-003, CT-EFF-005, CT-USE-007 -->
<script lang="ts" generics="Row extends LinkRow">
  // One titled list of trace rows, fetched a page at a time. Click or Enter follows an
  // element row, and opens a code row in the source view.
  import type { LinkRow, Page } from './api';
  import { onMount } from 'svelte';
  import type { Refresher } from './live';
  import { PagedRows } from './pages';
  import { codeTarget } from './find';
  import { followable } from './trace';
  import VirtualList from './VirtualList.svelte';

  interface Props {
    title: string;
    /** The element whose rows to show; the list starts again when it changes. */
    id: number;
    fetch: (offset: number, limit: number) => Promise<Page<Row>>;
    label: (row: Row) => string;
    onfollow: (id: number) => void;
    /** Opens a code row in the source view. */
    onopen: (path: string, line: number) => void;
    /** Fetches the rows again after an edit rebuilt the trace graph. */
    refresher?: Refresher;
  }
  let { title, id, fetch, label, onfollow, onopen, refresher }: Props = $props();

  let version = $state(0);
  let failure = $state('');
  const rows = new PagedRows<Row>(
    (offset, limit) => fetch(offset, limit),
    () => (version += 1),
    undefined,
    (e) => (failure = e instanceof Error ? e.message : String(e)),
  );
  const count = $derived.by(() => {
    void version;
    return rows.count;
  });

  $effect(() => {
    void id;
    failure = '';
    void rows.reset();
  });

  onMount(() => refresher?.subscribe(() => void rows.refresh()));

  function at(index: number) {
    void version;
    return rows.at(index);
  }

  function follow(index: number) {
    const row = rows.at(index);
    const target = row && followable(row.other);
    if (target !== undefined) {
      onfollow(target);
    } else if (row) {
      const code = codeTarget(row.other);
      if (code) {
        onopen(code.path, code.line);
      }
    }
  }
</script>

<h3>{title}{count > 0 ? ` (${count})` : ''}</h3>
{#if failure}
  <p class="problem" role="alert">{failure}</p>
{:else if count === 0}
  <p class="muted">{version > 0 ? 'None.' : 'Loading...'}</p>
{:else}
  <div class="list" style:height="{Math.min(count, 8) * 24 + 4}px">
    <VirtualList {count} label={title} rowAt={(i) => at(i)} onactivate={follow}>
      {#snippet row(r)}
        <span title="{r.file}:{r.line}">{label(r)}</span>
      {/snippet}
    </VirtualList>
  </div>
{/if}

<style>
  h3 {
    margin: 0.75rem 0 0.25rem;
    font-size: 0.9rem;
  }
  .list {
    min-height: 3rem;
  }
  .muted {
    margin: 0;
    color: var(--text-muted);
  }
  .problem {
    margin: 0;
    color: var(--danger);
  }
</style>
<!-- @end -->
