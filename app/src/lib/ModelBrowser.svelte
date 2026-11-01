<!-- @begin CT-MOD-005, CT-MOD-003, CT-EFF-005, CT-USE-004, CT-USE-007 -->
<script lang="ts">
  // The model tree: packages, definitions, and usages, loaded a page at a time as the user
  // opens a package or scrolls. Right opens, Left closes or moves to the parent, Enter selects.
  import { onMount } from 'svelte';
  import { treeChildren } from './api';
  import { treeLabel } from './describe';
  import type { Refresher } from './live';
  import { TreeModel } from './tree';
  import VirtualList from './VirtualList.svelte';

  let { onselect, refresher }: { onselect: (id: number) => void; refresher?: Refresher } = $props();

  let version = $state(0);
  let problem = $state('');
  const tree = new TreeModel(
    treeChildren,
    () => (version += 1),
    undefined,
    (e) => (problem = e instanceof Error ? e.message : String(e)),
  );
  const count = $derived.by(() => {
    void version;
    return tree.count;
  });
  let selected = $state(-1);

  // After an edit the tree is fetched again, keeping the open packages, and the selection
  // follows its element to the row it moved to.
  async function refresh() {
    const keep = tree.at(selected)?.row?.id;
    await tree.refresh();
    if (keep !== undefined) {
      selected = tree.indexOf(keep);
    }
  }

  onMount(() => {
    void tree.reset();
    return refresher?.subscribe(() => void refresh());
  });

  // A copy of the node, so the row is drawn again when the model changes it in place.
  function snapshot(index: number) {
    void version;
    const node = tree.at(index);
    return node && { ...node };
  }

  function pick(index: number) {
    const id = tree.at(index)?.row?.id;
    if (id !== undefined) {
      onselect(id);
    }
  }

  async function onkeydown(event: KeyboardEvent) {
    const node = tree.at(selected);
    if (!node) {
      return;
    }
    if (event.key === 'ArrowRight') {
      await tree.expand(selected);
    } else if (event.key === 'ArrowLeft') {
      if (node.expanded) {
        tree.collapse(selected);
      } else if (node.depth > 0) {
        selected = Math.max(0, tree.indexOf(node.parent));
        pick(selected);
      }
    } else if (event.key === 'Enter') {
      pick(selected);
    }
  }
</script>

{#if problem}
  <p class="problem" role="alert">{problem}</p>
{/if}
<!-- svelte-ignore a11y_no_static_element_interactions: the keys reach the listbox inside -->
<div class="browser" {onkeydown}>
  <VirtualList
    {count}
    label="Model tree"
    rowAt={(i) => snapshot(i)}
    bind:selected
    onselect={pick}
  >
    {#snippet row(node)}
      <span class="indent" style:width="{node.depth * 1}rem"></span>
      {#if node.row}
        <span
          class="twisty"
          aria-hidden="true"
          title={node.row.kind}
          onclick={(e) => { e.stopPropagation(); void tree.toggle(tree.indexOf(node.row?.id ?? -1)); }}
        >{node.row.children > 0 ? (node.expanded ? '▾' : '▸') : ''}</span>
        {treeLabel(node.row)}
      {:else}
        <span class="muted">Loading...</span>
      {/if}
    {/snippet}
  </VirtualList>
</div>

<style>
  .browser {
    height: calc(100% - 2rem);
    min-height: 6rem;
  }
  .indent,
  .twisty {
    display: inline-block;
  }
  .twisty {
    width: 1rem;
    cursor: pointer;
  }
  .muted {
    color: var(--text-muted);
  }
  .problem {
    color: var(--danger);
  }
</style>
<!-- @end -->
