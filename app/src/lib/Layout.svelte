<script lang="ts">
  // @begin CT-USE-004, CT-EFF-005
  // The pane layout: header, model browser, main view, trace panel, and problems. Each pane
  // is a landmark, and the DOM order is the keyboard focus order. Panes get their content
  // from the parent, so later tasks fill them without touching the layout.
  import type { Snippet } from 'svelte';

  let {
    header,
    browser,
    main,
    trace,
    problems,
  }: {
    header: Snippet;
    browser: Snippet;
    main: Snippet;
    trace: Snippet;
    problems: Snippet;
  } = $props();
  // @end
</script>

<!-- @begin CT-USE-004, CT-EFF-005 -->
<a class="skip" href="#main">Skip to main view</a>
<div class="shell">
  <header>{@render header()}</header>
  <nav aria-label="Model browser" class="browser">{@render browser()}</nav>
  <main id="main" tabindex="-1">{@render main()}</main>
  <aside aria-label="Trace panel" class="trace">{@render trace()}</aside>
  <section aria-label="Problems" class="problems">{@render problems()}</section>
</div>
<!-- @end -->

<style>
  /* @begin CT-USE-004, CT-USE-005 */
  .skip {
    position: absolute;
    left: -999px;
    top: 0;
    padding: 0.5rem 1rem;
    background: var(--surface);
    color: var(--fg);
    z-index: 30;
  }
  .skip:focus {
    left: 0;
  }
  .shell {
    display: grid;
    grid-template-columns: minmax(12rem, 20rem) minmax(0, 1fr) minmax(12rem, 22rem);
    grid-template-rows: auto minmax(0, 1fr) minmax(5rem, 12rem);
    grid-template-areas:
      'head head head'
      'browser main trace'
      'problems problems problems';
    height: 100vh;
  }
  header {
    grid-area: head;
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.4rem 0.75rem;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .browser {
    grid-area: browser;
    border-right: 1px solid var(--border);
  }
  main {
    grid-area: main;
    overflow: auto;
    padding: 0.75rem;
  }
  main:focus {
    outline: none;
  }
  .trace {
    grid-area: trace;
    border-left: 1px solid var(--border);
  }
  .problems {
    grid-area: problems;
    border-top: 1px solid var(--border);
  }
  .browser,
  .trace,
  .problems {
    overflow: auto;
    padding: 0.5rem 0.75rem;
    background: var(--surface);
  }
  @media (max-width: 800px) {
    .shell {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto auto minmax(0, 1fr) auto auto;
      grid-template-areas: 'head' 'browser' 'main' 'trace' 'problems';
      height: auto;
      min-height: 100vh;
    }
  }
  /* @end */
</style>
