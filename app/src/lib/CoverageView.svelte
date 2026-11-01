<!-- @begin CT-TRC-004, CT-EFF-005, CT-USE-007 -->
<script lang="ts">
  // The coverage view: requirements with no parent, implementation, or verification, and
  // elements traced to no requirement, one filter at a time. A click selects the element.
  import { untrack } from 'svelte';
  import { coverageGaps, type GapFilter, type GapRow } from './api';
  import { filterLabel, GAP_FILTERS, gapLabel } from './find';
  import { PagedRows } from './pages';
  import VirtualList from './VirtualList.svelte';

  interface Props {
    filter: GapFilter;
    counts?: Partial<Record<GapFilter, number>>;
    onfilter: (filter: GapFilter) => void;
    onselect: (id: number) => void;
  }
  let { filter, counts, onfilter, onselect }: Props = $props();

  let version = $state(0);
  let failure = $state('');
  // The filter whose first page has arrived. The in-app probe waits for it.
  let ready = $state<GapFilter>();
  let current: GapFilter = untrack(() => filter);
  const rows = new PagedRows<GapRow>(
    (offset, limit) => coverageGaps(current, offset, limit),
    () => (version += 1),
    undefined,
    (e) => (failure = e instanceof Error ? e.message : String(e)),
  );
  const count = $derived.by(() => {
    void version;
    return rows.count;
  });

  $effect(() => {
    const wanted = filter;
    current = wanted;
    failure = '';
    ready = undefined;
    void rows.reset().then(() => {
      if (current === wanted) {
        ready = wanted;
      }
    });
  });

  function at(index: number) {
    void version;
    return rows.at(index);
  }

  function pick(index: number) {
    const row = rows.at(index);
    if (row) {
      onselect(row.id);
    }
  }
</script>

<div class="view" data-view="coverage" data-filter={filter} data-ready={ready === filter ? '' : undefined}>
  <div class="bar">
    {#each GAP_FILTERS as f (f.id)}
      <button type="button" aria-pressed={filter === f.id} onclick={() => onfilter(f.id)}>{filterLabel(f, counts)}</button>
    {/each}
  </div>
  {#if failure}
    <p class="problem" role="alert">{failure}</p>
  {:else if count === 0}
    <p class="muted">{ready === filter ? 'Nothing to show: no element has this gap.' : 'Loading...'}</p>
  {:else}
    <div class="list">
      <VirtualList {count} label="Coverage list" rowAt={(i) => at(i)} onactivate={pick}>
        {#snippet row(r)}
          <span title="{r.file}:{r.line}">{gapLabel(r)}</span>
        {/snippet}
      </VirtualList>
    </div>
  {/if}
</div>

<style>
  .view {
    display: flex;
    flex-direction: column;
    height: calc(100% - 2rem);
    min-height: 8rem;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    margin-bottom: 0.5rem;
  }
  button[aria-pressed='true'] {
    background: var(--row-selected);
  }
  .list {
    flex: 1;
    min-height: 5rem;
  }
  .muted {
    color: var(--text-muted);
  }
  .problem {
    color: var(--danger);
  }
</style>
<!-- @end -->
