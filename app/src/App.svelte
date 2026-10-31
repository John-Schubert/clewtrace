<script lang="ts">
  // @begin CT-PLT-001, CT-EFF-004, CT-MOD-005
  // The app window: shows the app name and version, read from the Rust side, and the
  // project opened from the command line once its background load ends.
  import { getName, getVersion } from '@tauri-apps/api/app';
  import {
    clearRecent,
    describeStatus,
    type ProjectStatus,
    openProject,
    pickFolder,
    recentProjects,
    waitForProject,
    type RecentProject,
  } from './lib/api';
  import { ActionRegistry } from './lib/actions';
  import DetailView from './lib/DetailView.svelte';
  import ModelBrowser from './lib/ModelBrowser.svelte';
  import ProblemsPanel from './lib/ProblemsPanel.svelte';
  import type { Selection } from './lib/describe';
  import Tokens from './lib/Tokens.svelte';
  import CommandPalette from './lib/CommandPalette.svelte';
  import { isPaletteShortcut, nextPane, PANES, type Pane } from './shell';
  import {
    applyTheme,
    loadPreference,
    resolveTheme,
    savePreference,
    type ThemePreference,
  } from './lib/theme';

  let name = $state('Clewtrace');
  let version = $state('');
  let project = $state('');
  // Counts the projects opened. The browser and problems panes start again for each one.
  let opened = $state(0);
  let selection = $state<Selection>();

  function showStatus(status: ProjectStatus) {
    project = describeStatus(status);
    selection = undefined;
    if (status.state === 'ready') {
      opened += 1;
    }
  }

  Promise.all([getName(), getVersion()]).then(([n, v]) => {
    name = n;
    version = v;
  });
  waitForProject().then(showStatus);
  // @end

  // @begin CT-SEC-003, CT-USE-007
  // Opening a project: the folder dialog, the recent list, and its "Clear recent" action.
  // The list holds folder paths only; the Rust side keeps it in the app config folder.
  let recent = $state<RecentProject[]>([]);
  let problem = $state('');

  function show(error: unknown) {
    problem = error instanceof Error ? error.message : String(error);
  }

  async function reloadRecent() {
    recent = await recentProjects();
  }

  async function open(path: string) {
    problem = '';
    project = `Opening ${path}`;
    try {
      showStatus(await openProject(path));
      await reloadRecent();
    } catch (e) {
      show(e);
    }
  }

  async function choose() {
    try {
      const path = await pickFolder();
      if (path) {
        await open(path);
      }
    } catch (e) {
      show(e);
    }
  }

  async function clear() {
    try {
      await clearRecent();
      recent = [];
    } catch (e) {
      show(e);
    }
  }

  reloadRecent().catch(show);
  // @end

  // @begin CT-USE-005
  // Theme: the OS setting by default, a manual override stored as a preference.
  let preference = $state<ThemePreference>(loadPreference());
  let systemDark = $state(window.matchMedia('(prefers-color-scheme: dark)').matches);
  const darkQuery = window.matchMedia('(prefers-color-scheme: dark)');

  $effect(() => {
    const onchange = (e: MediaQueryListEvent) => (systemDark = e.matches);
    darkQuery.addEventListener('change', onchange);
    return () => darkQuery.removeEventListener('change', onchange);
  });
  $effect(() => applyTheme(document.documentElement, resolveTheme(preference, systemDark)));

  function setTheme(next: ThemePreference) {
    preference = next;
    savePreference(next);
  }
  // @end

  // @begin CT-USE-002, CT-USE-004
  // The action registry and command palette, and the pane focus order (F6).
  const registry = new ActionRegistry();
  let paletteOpen = $state(false);
  let current = $state<Pane>();
  const paneTitles: Record<Pane, string> = {
    browser: 'Model browser',
    main: 'Main view',
    trace: 'Trace panel',
    problems: 'Problems',
  };

  function focusPane(pane: Pane) {
    current = pane;
    document.getElementById(`pane-${pane}`)?.focus();
  }

  registry.register({ id: 'project.open', title: 'Open project folder...', run: choose });
  registry.register({ id: 'project.clearRecent', title: 'Clear recent projects', run: clear });
  registry.register({ id: 'view.theme.system', title: 'Theme: follow the OS setting', run: () => setTheme('system') });
  registry.register({ id: 'view.theme.light', title: 'Theme: light', run: () => setTheme('light') });
  registry.register({ id: 'view.theme.dark', title: 'Theme: dark', run: () => setTheme('dark') });
  for (const pane of PANES) {
    registry.register({ id: `view.focus.${pane}`, title: `Go to ${paneTitles[pane]}`, run: () => focusPane(pane) });
  }
  registry.register({
    id: 'view.palette',
    title: 'Show all commands',
    shortcut: 'Ctrl+Shift+P',
    run: () => {
      paletteOpen = true;
    },
  });

  function onwindowkeydown(event: KeyboardEvent) {
    if (isPaletteShortcut(event)) {
      event.preventDefault();
      paletteOpen = true;
    } else if (event.key === 'F6') {
      event.preventDefault();
      focusPane(nextPane(current, event.shiftKey));
    }
  }

  async function runAction(id: string) {
    paletteOpen = false;
    try {
      await registry.run(id);
    } catch (e) {
      show(e);
    }
  }
  // @end
</script>

<!-- @begin CT-USE-002, CT-USE-004 -->
<svelte:window onkeydown={onwindowkeydown} />
<!-- @end -->

<!-- @begin CT-PLT-001, CT-EFF-004, CT-SEC-003, CT-USE-007, CT-USE-004, CT-MOD-005 -->
<Tokens />
<a class="skip" href="#pane-main">Skip to main view</a>
<div class="shell">
  <header>
    <h1>{name}</h1>
    {#if version}<span class="version">Version {version}</span>{/if}
    <button type="button" onclick={() => (paletteOpen = true)}>Commands (Ctrl+Shift+P)</button>
    <label>
      Theme
      <select value={preference} onchange={(e) => setTheme(e.currentTarget.value as ThemePreference)}>
        <option value="system">Follow the OS</option>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
    </label>
  </header>
  <nav id="pane-browser" tabindex="-1" aria-label="Model browser">
    <h2>Model browser</h2>
    {#if opened > 0}
      {#key opened}
        <ModelBrowser onselect={(id) => (selection = { kind: 'element', id })} />
      {/key}
    {/if}
    {#if recent.length > 0}
      <h3>Recent projects</h3>
      <ul>
        {#each recent as r (r.path)}
          <li><button type="button" title={r.path} onclick={() => open(r.path)}>{r.name}</button></li>
        {/each}
      </ul>
      <button type="button" onclick={clear}>Clear recent</button>
    {/if}
  </nav>
  <main id="pane-main" tabindex="-1" aria-label="Main view">
    {#if project}
      <p class="project">{project}</p>
    {/if}
    <p><button type="button" onclick={choose}>Open project folder...</button></p>
    {#if problem}
      <p class="problem" role="alert">{problem}</p>
    {/if}
    {#if selection}
      <DetailView {selection} />
    {/if}
  </main>
  <aside id="pane-trace" tabindex="-1" aria-label="Trace panel">
    <h2>Trace panel</h2>
    <p class="muted">Links to and from the selected element arrive in task 2.7a.</p>
  </aside>
  <section id="pane-problems" tabindex="-1" aria-label="Problems">
    <h2>Problems</h2>
    {#if opened > 0}
      {#key opened}
        <ProblemsPanel onselect={(p) => (selection = { kind: 'problem', problem: p })} />
      {/key}
    {/if}
  </section>
</div>
{#if paletteOpen}
  <CommandPalette actions={registry.list()} onrun={runAction} onclose={() => { paletteOpen = false; if (current) focusPane(current); }} />
{/if}
<!-- @end -->

<style>
  /* @begin CT-PLT-001, CT-USE-005, CT-USE-004 */
  :global(body) {
    margin: 0;
    font-family: system-ui, sans-serif;
    color: var(--text);
    background: var(--bg);
  }
  :global(:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .skip {
    position: absolute;
    left: -999px;
  }
  .skip:focus {
    left: 0.5rem;
    top: 0.5rem;
    z-index: 10;
    padding: 0.4rem 0.75rem;
    background: var(--surface);
  }
  .shell {
    display: grid;
    grid-template: 'head head head' auto 'browser main trace' 1fr 'problems problems problems' 10rem / 16rem 1fr 18rem;
    height: 100vh;
  }
  header {
    grid-area: head;
    display: flex;
    gap: 1rem;
    align-items: center;
    padding: 0.4rem 0.75rem;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  header h1 {
    margin: 0;
    font-size: 1.1rem;
  }
  .version {
    flex: 1;
    color: var(--text-muted);
  }
  nav,
  main,
  aside,
  section {
    padding: 0.5rem 0.75rem;
    overflow: auto;
    border: 1px solid var(--border);
    margin: -1px 0 0 -1px;
  }
  nav {
    grid-area: browser;
  }
  main {
    grid-area: main;
  }
  aside {
    grid-area: trace;
  }
  section {
    grid-area: problems;
  }
  h2 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
  }
  h3 {
    margin: 1rem 0 0.25rem;
    font-size: 0.9rem;
  }
  p {
    margin: 0.5rem 0 0;
  }
  .muted {
    color: var(--text-muted);
  }
  button,
  select {
    font: inherit;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.2rem 0.6rem;
  }
  ul {
    margin: 0.25rem 0;
    padding: 0;
    list-style: none;
  }
  .project {
    max-width: 40rem;
  }
  .problem {
    max-width: 40rem;
    color: var(--danger);
  }
  /* @end */
</style>
