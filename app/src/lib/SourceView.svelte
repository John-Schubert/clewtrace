<!-- @begin CT-TRC-004, CT-EFF-005, CT-USE-007 -->
<script lang="ts">
  // A read-only view of a project file, opened at one line. Lines come a page at a time,
  // so a long file is never held whole.
  import { untrack } from 'svelte';
  import { sourceLines, type SourceLine } from './api';
  import { sourceLabel } from './find';
  import { PagedRows } from './pages';
  import VirtualList from './VirtualList.svelte';

  let { path, line }: { path: string; line: number } = $props();

  let version = $state(0);
  let failure = $state('');
  let selected = $state(-1);
  let current = untrack(() => path);
  const rows = new PagedRows<SourceLine>(
    (offset, limit) => sourceLines(current, offset, limit),
    () => (version += 1),
    undefined,
    (e) => (failure = e instanceof Error ? e.message : String(e)),
  );
  const count = $derived.by(() => {
    void version;
    return rows.count;
  });

  $effect(() => {
    current = path;
    failure = '';
    selected = -1;
    void rows.reset();
  });

  // Once the file's size is known, select the line, which scrolls it into view.
  $effect(() => {
    if (count > 0) {
      selected = Math.min(Math.max(line, 1), count) - 1;
    }
  });

  function at(index: number) {
    void version;
    return rows.at(index);
  }
</script>

<div class="view" data-view="source">
  <h2>{path}:{line}</h2>
  {#if failure}
    <p class="problem" role="alert">{failure}</p>
  {:else if count === 0}
    <p class="muted">{version > 0 ? 'The file is empty.' : 'Loading...'}</p>
  {:else}
    <div class="list">
      <VirtualList {count} label="Source lines" rowAt={(i) => at(i)} bind:selected>
        {#snippet row(r)}
          <code>{sourceLabel(r)}</code>
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
  h2 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
    overflow-wrap: anywhere;
  }
  code {
    white-space: pre;
    font-family: ui-monospace, monospace;
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
