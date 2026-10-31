<script lang="ts">
  // @begin CT-USE-002, CT-USE-004
  // The command palette: a dialog with a search box and the matching actions from the
  // registry. Type to filter, arrow keys to move, Enter to run, Escape to close.
  import { registry, type Action } from './actions';
  import { moveIndex } from './virtual';

  let { open = $bindable(false) }: { open: boolean } = $props();

  let query = $state('');
  let active = $state(0);
  let input: HTMLInputElement | undefined = $state();
  let returnTo: HTMLElement | null = null;

  const matches = $derived(open ? registry.filter(query) : []);

  $effect(() => {
    if (open) {
      returnTo = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      query = '';
      active = 0;
      input?.focus();
    } else if (returnTo) {
      returnTo.focus();
      returnTo = null;
    }
  });

  $effect(() => {
    // A new query puts the first match under the cursor.
    void query;
    active = 0;
  });

  async function run(action: Action | undefined) {
    if (!action) {
      return;
    }
    open = false;
    await action.run();
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      open = false;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      void run(matches[active]);
    } else if (e.key === 'Tab') {
      // The search box is the only stop in the dialog.
      e.preventDefault();
    } else {
      const next = moveIndex(active, e.key, matches.length, 8);
      if (next !== active) {
        e.preventDefault();
        active = next;
      }
    }
  }
  // @end
</script>

<!-- @begin CT-USE-002, CT-USE-004 -->
{#if open}
  <div class="backdrop" role="presentation" onclick={() => (open = false)}>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={keydown}
    >
      <input
        bind:this={input}
        bind:value={query}
        type="text"
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-activedescendant={matches.length > 0 ? `palette-${active}` : undefined}
        aria-label="Type a command"
        placeholder="Type a command"
        autocomplete="off"
        spellcheck="false"
      />
      <ul id="palette-list" role="listbox" aria-label="Commands">
        {#each matches as action, i (action.id)}
          <!-- The search box owns the keyboard (aria-activedescendant), so rows need no key handler. -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <li
            id="palette-{i}"
            role="option"
            aria-selected={i === active}
            onclick={() => run(action)}
          >
            <span>{action.title}</span>
            {#if action.shortcut}<kbd>{action.shortcut}</kbd>{/if}
          </li>
        {:else}
          <li class="none" role="presentation">No matching command</li>
        {/each}
      </ul>
    </div>
  </div>
{/if}
<!-- @end -->

<style>
  /* @begin CT-USE-002, CT-USE-005 */
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    background: var(--backdrop);
  }
  .dialog {
    width: min(36rem, 92vw);
    background: var(--surface);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    box-shadow: 0 8px 28px var(--backdrop);
  }
  input {
    box-sizing: border-box;
    width: 100%;
    padding: 0.7rem 0.9rem;
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
    margin: 0;
    padding: 0.25rem;
    max-height: 50vh;
    overflow-y: auto;
    list-style: none;
  }
  li {
    display: flex;
    justify-content: space-between;
    padding: 0.45rem 0.65rem;
    border-radius: 0.3rem;
    cursor: default;
  }
  li[aria-selected='true'] {
    background: var(--selected-bg);
    color: var(--selected-fg);
  }
  .none {
    color: var(--muted);
  }
  kbd {
    font-family: inherit;
    font-size: 0.85em;
    color: var(--muted);
  }
  /* @end */
</style>
