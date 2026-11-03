<!-- @begin CT-EFF-005, CT-USE-007, CT-TRC-004, CT-MOD-004, CT-MOD-003, CT-USE-010 -->
<script lang="ts">
  import { tip } from './tooltip';
  // The main view's detail of what is selected: an element from the tree, or a problem from
  // the problems list.
  import { elementDetail, elementReferences, type ElementDetail, type Page, type ReferenceRow } from './api';
  import { detailFields, problemSummary, referenceLabel, referencesNote, type Selection } from './describe';
  import { onMount } from 'svelte';
  import { canEdit, placeOf } from './editor';
  import type { Refresher } from './live';

  let {
    selection,
    onopen,
    refresher,
  }: { selection: Selection; onopen: (path: string, line: number) => void; refresher?: Refresher } = $props();

  let detail = $state<ElementDetail>();
  let failure = $state('');
  let references = $state<Page<ReferenceRow>>();

  // Fetches the element again. Keeps what is shown until the answer arrives, so an edit does
  // not blank the view. An element that an edit removed gives its error.
  function load(current: Selection, keep = false) {
    if (!keep) {
      detail = undefined;
      references = undefined;
    }
    failure = '';
    if (current.kind === 'element') {
      elementReferences(current.id)
        .then((r) => {
          if (selection === current) {
            references = r;
          }
        })
        .catch(() => {
          references = undefined;
        });
      elementDetail(current.id)
        .then((d) => {
          if (selection === current) {
            detail = d;
          }
        })
        .catch((e: unknown) => {
          if (selection === current) {
            detail = undefined;
            failure = e instanceof Error ? e.message : String(e);
          }
        });
    }
  }

  $effect(() => {
    load(selection);
  });

  onMount(() => refresher?.subscribe(() => load(selection, true)));
</script>

{#if failure}
  <p class="problem" role="alert">{failure}</p>
{:else if selection.kind === 'problem'}
  <h2>{selection.problem.severity === 'error' ? 'Error' : 'Warning'}</h2>
  <dl>
    <dt>Where</dt>
    <dd>{selection.problem.where || '(no place)'}</dd>
    <dt>What went wrong</dt>
    <dd>{selection.problem.what}</dd>
    <dt>How to fix it</dt>
    <dd>{selection.problem.help}</dd>
  </dl>
  <p class="muted">{problemSummary(selection.problem)}</p>
  {#if placeOf(selection.problem.where)}
    {@const place = placeOf(selection.problem.where)!}
    <p><button type="button" use:tip={'Open the file at the element'} onclick={() => onopen(place.path, place.line)}>{canEdit(place.path) ? 'Open in editor' : 'Show in source view'}</button></p>
  {/if}
{:else if detail}
  <h2>{detail.shortName ?? detail.name ?? '(unnamed)'}</h2>
  <dl>
    {#each detailFields(detail) as field (field.label)}
      <dt>{field.label}</dt>
      <dd>{field.value}</dd>
    {/each}
  </dl>
  {#if references}
    <h3>References</h3>
    <p class="muted">{referencesNote(references.rows.length, references.total ?? references.rows.length)}</p>
    <ul>
      {#each references.rows as row (`${row.file}:${row.line}:${row.column}`)}
        <li><button type="button" class="link" use:tip={`${row.file}:${row.line}`} onclick={() => onopen(row.file, row.line)}>{referenceLabel(row)}</button></li>
      {/each}
    </ul>
  {/if}
  <p><button type="button" use:tip={'Open the file at the element'} onclick={() => onopen(detail!.file, detail!.line)}>{canEdit(detail.file) ? 'Open in editor' : 'Show in source view'}</button></p>
{:else}
  <p class="muted">Loading...</p>
{/if}

<style>
  dl {
    margin: 0;
    max-width: 48rem;
  }
  dt {
    margin-top: 0.5rem;
    font-size: 0.85rem;
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  h3 {
    margin: 1rem 0 0.25rem;
    font-size: 0.95rem;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .link {
    border: none;
    background: none;
    padding: 0.125rem 0;
    color: var(--accent);
    text-align: left;
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
