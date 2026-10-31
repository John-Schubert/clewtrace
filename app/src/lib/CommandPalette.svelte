<!-- @begin CT-USE-002, CT-USE-004 -->
<script lang="ts">
  // The command palette: type to filter the registered actions, arrows to move, Enter to run,
  // Escape to close. Opened with Ctrl+Shift+P (Cmd+Shift+P on macOS) or Ctrl+K.
  import { filterActions, type Action } from './actions';

  interface Props {
    actions: Action[];
    onrun: (id: string) => void;
    onclose: () => void;
  }
  let { actions, onrun, onclose }: Props = $props();

  let query = $state('');
  let active = $state(0);
  const hits = $derived(filterActions(actions, query));

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onclose();
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const step = event.key === 'ArrowDown' ? 1 : -1;
      active = hits.length === 0 ? 0 : (active + step + hits.length) % hits.length;
    } else if (event.key === 'Enter' && hits[active]) {
      event.preventDefault();
      onrun(hits[active].id);
    }
  }
</script>

<div class="backdrop" role="presentation" onclick={onclose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Command palette" tabindex="-1" {onkeydown} onclick={(e) => e.stopPropagation()}>
    <input
      type="text"
      role="combobox"
      aria-label="Type a command"
      aria-expanded="true"
      aria-controls="palette-list"
      aria-activedescendant={hits[active] ? `palette-${hits[active].id}` : undefined}
      placeholder="Type a command"
      bind:value={query}
      oninput={() => (active = 0)}
      use:focusOnMount
    />
    <ul id="palette-list" role="listbox" aria-label="Commands">
      {#each hits as hit, i (hit.id)}
        <li id="palette-{hit.id}" role="option" aria-selected={i === active} class:active={i === active}>
          <button type="button" tabindex="-1" onclick={() => onrun(hit.id)}>
            <span>{hit.title}</span>
            {#if hit.shortcut}<kbd>{hit.shortcut}</kbd>{/if}
          </button>
        </li>
      {:else}
        <li class="none" role="option" aria-selected="false" aria-disabled="true">No matching command</li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    justify-items: center;
    align-items: start;
    padding-top: 12vh;
    background: rgb(0 0 0 / 0.4);
  }
  .dialog {
    width: min(36rem, 90vw);
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  input {
    box-sizing: border-box;
    width: 100%;
    padding: 0.6rem 0.75rem;
    font: inherit;
    color: inherit;
    background: transparent;
    border: 0;
    border-bottom: 1px solid var(--border);
  }
  input:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  ul {
    max-height: 50vh;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }
  li button {
    display: flex;
    justify-content: space-between;
    width: 100%;
    padding: 0.4rem 0.75rem;
    font: inherit;
    color: inherit;
    text-align: left;
    background: transparent;
    border: 0;
  }
  li.active button {
    background: var(--row-selected);
  }
  kbd {
    color: var(--text-muted);
  }
  .none {
    padding: 0.4rem 0.75rem;
    color: var(--text-muted);
  }
</style>
<!-- @end -->
