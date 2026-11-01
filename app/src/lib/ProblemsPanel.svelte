<!-- @begin CT-USE-007, CT-MOD-003, CT-EFF-005, CT-USE-004 -->
<script lang="ts">
  // Every syntax, name, and trace problem as file:line:column, what went wrong, and the
  // fix hint. Errors come first, then warnings. Click or arrow to select a problem.
  import { onMount } from 'svelte';
  import { problems, type ProblemRow } from './api';
  import { problemSummary } from './describe';
  import type { Refresher } from './live';
  import { PagedRows } from './pages';
  import VirtualList from './VirtualList.svelte';

  let { onselect, refresher }: { onselect: (problem: ProblemRow) => void; refresher?: Refresher } = $props();

  let version = $state(0);
  let failure = $state('');
  const rows = new PagedRows(
    problems,
    () => (version += 1),
    undefined,
    (e) => (failure = e instanceof Error ? e.message : String(e)),
  );
  const count = $derived.by(() => {
    void version;
    return rows.count;
  });

  onMount(() => {
    void rows.reset();
    return refresher?.subscribe(() => void rows.refresh());
  });

  function at(index: number) {
    void version;
    return rows.at(index);
  }

  function pick(index: number) {
    const problem = rows.at(index);
    if (problem) {
      onselect(problem);
    }
  }
</script>

{#if failure}
  <p class="problem" role="alert">{failure}</p>
{:else if count === 0 && version > 0}
  <p class="muted">No problems.</p>
{:else}
  <div class="problems">
    <VirtualList {count} label="Problems list" rowAt={(i) => at(i)} onselect={pick}>
      {#snippet row(p)}
        <span class={p.severity} title={problemSummary(p)}>
          <strong>{p.severity}</strong>
          {p.where}: {p.what}. Fix: {p.help}
        </span>
      {/snippet}
    </VirtualList>
  </div>
{/if}

<style>
  .problems {
    height: calc(100% - 2rem);
    min-height: 3rem;
  }
  .error strong {
    color: var(--danger);
  }
  .warning strong {
    color: var(--warning, var(--text-muted));
  }
  .muted {
    color: var(--text-muted);
  }
  .problem {
    color: var(--danger);
  }
</style>
<!-- @end -->
