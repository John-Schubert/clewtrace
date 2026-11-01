<!-- @begin CT-TRC-005, CT-TRC-006, CT-EFF-005 -->
<script lang="ts">
  // The trace panel: links to and from the selected element, grouped by kind, and an impact
  // view of everything upstream and downstream. Back and forward step through the elements
  // looked at.
  import { elementImpact, elementLinks } from './api';
  import { impactLabel, linkLabel } from './trace';
  import TraceList from './TraceList.svelte';

  interface Props {
    id: number | undefined;
    canBack: boolean;
    canForward: boolean;
    onfollow: (id: number) => void;
    onback: () => void;
    onforward: () => void;
  }
  let { id, canBack, canForward, onfollow, onback, onforward }: Props = $props();

  let tab = $state<'links' | 'impact'>('links');
</script>

<div class="bar">
  <button type="button" disabled={!canBack} onclick={onback}>Back</button>
  <button type="button" disabled={!canForward} onclick={onforward}>Forward</button>
  <button type="button" aria-pressed={tab === 'links'} onclick={() => (tab = 'links')}>Links</button>
  <button type="button" aria-pressed={tab === 'impact'} onclick={() => (tab = 'impact')}>Impact</button>
</div>
{#if id === undefined}
  <p class="muted">Select an element to see its trace links.</p>
{:else if tab === 'links'}
  <TraceList title="Traces to" {id} fetch={(o, l) => elementLinks(id, 'from', o, l)} label={linkLabel} {onfollow} />
  <TraceList title="Traced by" {id} fetch={(o, l) => elementLinks(id, 'to', o, l)} label={linkLabel} {onfollow} />
{:else}
  <TraceList title="Upstream" {id} fetch={(o, l) => elementImpact(id, 'upstream', o, l)} label={impactLabel} {onfollow} />
  <TraceList title="Downstream" {id} fetch={(o, l) => elementImpact(id, 'downstream', o, l)} label={impactLabel} {onfollow} />
{/if}

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
  }
  button[aria-pressed='true'] {
    background: var(--row-selected);
  }
  .muted {
    color: var(--text-muted);
  }
</style>
<!-- @end -->
