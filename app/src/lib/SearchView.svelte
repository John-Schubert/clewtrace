<!-- @begin CT-EFF-006, CT-EFF-005, CT-TRC-004, CT-USE-007 -->
<script lang="ts">
  // Full-text search over the model: a box, and the hits a page at a time. A click selects
  // the element of a hit, or opens its file when the model has no element for it.
  import { onMount, untrack } from 'svelte';
  import { search, type Page, type SearchHit } from './api';
  import { hitLabel, hitTarget } from './find';
  import { PagedRows } from './pages';
  import VirtualList from './VirtualList.svelte';

  interface Props {
    query: string;
    onquery: (query: string) => void;
    onselect: (id: number) => void;
    onopen: (path: string, line: number) => void;
  }
  let { query, onquery, onselect, onopen }: Props = $props();

  let input = $state<HTMLInputElement>();
  let version = $state(0);
  let failure = $state('');
  // The query whose first page has arrived. The in-app probe waits for it.
  let ready = $state<string>();
  let current = untrack(() => query);
  const blank = (text: string) => text.trim() === '';
  const empty = (offset: number): Promise<Page<SearchHit>> => Promise.resolve({ offset, total: 0, more: false, rows: [] });
  const rows = new PagedRows<SearchHit>(
    (offset, limit) => (blank(current) ? empty(offset) : search(current, offset, limit)),
    () => (version += 1),
    undefined,
    (e) => (failure = e instanceof Error ? e.message : String(e)),
  );
  const count = $derived.by(() => {
    void version;
    return rows.count;
  });

  onMount(() => input?.focus());

  $effect(() => {
    const wanted = query;
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
    const hit = rows.at(index);
    if (!hit) {
      return;
    }
    const target = hitTarget(hit);
    if (target.kind === 'element') {
      onselect(target.id);
    } else {
      onopen(target.path, target.line);
    }
  }
</script>

<div class="view" data-view="search" data-query={query} data-ready={ready === query ? '' : undefined}>
  <input
    id="search-input"
    type="search"
    aria-label="Search the model"
    placeholder="Search requirement text, names, and IDs"
    value={query}
    oninput={(e) => onquery(e.currentTarget.value)}
    bind:this={input}
  />
  {#if failure}
    <p class="problem" role="alert">{failure}</p>
  {:else if blank(query)}
    <p class="muted">Type to search.</p>
  {:else if count === 0}
    <p class="muted">{ready === query ? 'No matches.' : 'Searching...'}</p>
  {:else}
    <div class="list">
      <VirtualList {count} label="Search results" rowAt={(i) => at(i)} onactivate={pick}>
        {#snippet row(hit)}
          <span title="{hit.file}:{hit.line}">{hitLabel(hit)}</span>
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
  input {
    font: inherit;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.3rem 0.5rem;
    margin-bottom: 0.5rem;
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
