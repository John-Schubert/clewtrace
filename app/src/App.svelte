<script lang="ts">
  // @begin CT-PLT-001, CT-EFF-004
  // The app window: shows the app name and version, read from the Rust side, and the
  // project opened from the command line once its background load ends.
  import { getName, getVersion } from '@tauri-apps/api/app';
  import { isPaletteKey, registry } from './lib/actions';
  import CommandPalette from './lib/CommandPalette.svelte';
  import Layout from './lib/Layout.svelte';
  import { applyTheme, browserStore, loadTheme, saveTheme, type ThemePref } from './lib/theme';
  import {
    clearRecent,
    describeStatus,
    openProject,
    pickFolder,
    recentProjects,
    waitForProject,
    type RecentProject,
  } from './lib/api';

  let name = $state('Clewtrace');
  let version = $state('');
  let project = $state('');

  Promise.all([getName(), getVersion()]).then(([n, v]) => {
    name = n;
    version = v;
  });
  waitForProject().then((status) => {
    project = describeStatus(status);
  });
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
      project = describeStatus(await openProject(path));
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
  // The theme: follows the OS setting unless the user picks one. The OS setting is watched
  // with a media query event, never polled.
  const osDark = window.matchMedia('(prefers-color-scheme: dark)');
  let themePref = $state<ThemePref>(loadTheme(browserStore()));

  function setTheme(pref: ThemePref) {
    themePref = pref;
    saveTheme(browserStore(), pref);
  }

  $effect(() => {
    const apply = () => applyTheme(document.documentElement, themePref, osDark.matches);
    apply();
    osDark.addEventListener('change', apply);
    return () => osDark.removeEventListener('change', apply);
  });
  // @end

  // @begin CT-USE-002
  // The command palette and the app's own actions. Later tasks register theirs the same way.
  let paletteOpen = $state(false);

  $effect(() => {
    const offs = [
      registry.register({ id: 'palette.open', title: 'Show all commands', shortcut: 'Ctrl+K', run: () => void (paletteOpen = true) }),
      registry.register({ id: 'project.open', title: 'Open project folder', keywords: 'folder browse', run: choose }),
      registry.register({ id: 'project.clearRecent', title: 'Clear recent projects', keywords: 'forget history', run: clear }),
      registry.register({ id: 'theme.system', title: 'Theme: follow the OS', keywords: 'appearance', run: () => setTheme('system') }),
      registry.register({ id: 'theme.light', title: 'Theme: light', keywords: 'appearance', run: () => setTheme('light') }),
      registry.register({ id: 'theme.dark', title: 'Theme: dark', keywords: 'appearance', run: () => setTheme('dark') }),
    ];
    return () => offs.forEach((off) => off());
  });

  function keydown(e: KeyboardEvent) {
    if (isPaletteKey(e)) {
      e.preventDefault();
      paletteOpen = !paletteOpen;
    }
  }
  // @end
</script>

<!-- @begin CT-USE-002 -->
<svelte:window onkeydown={keydown} />
<!-- @end -->

<!-- @begin CT-PLT-001, CT-EFF-004, CT-USE-004, CT-USE-002, CT-USE-005 -->
<Layout>
  {#snippet header()}
    <strong>{name}</strong>
    {#if version}<span class="muted">Version {version}</span>{/if}
    <span class="project">{project}</span>
    <button type="button" onclick={() => (paletteOpen = true)}>Commands (Ctrl+K)</button>
    <label>
      Theme
      <select value={themePref} onchange={(e) => setTheme(e.currentTarget.value as ThemePref)}>
        <option value="system">Follow the OS</option>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
    </label>
  {/snippet}
  {#snippet browser()}
    <h2>Model browser</h2>
    <p class="muted">The model tree arrives in a later task.</p>
  {/snippet}
  {#snippet main()}
    <h1>Project</h1>
    <p><button type="button" onclick={choose}>Open project folder...</button></p>
    {#if problem}
      <p class="problem" role="alert">{problem}</p>
    {/if}
    {#if recent.length > 0}
      <section aria-labelledby="recent-heading">
        <h2 id="recent-heading">Recent projects</h2>
        <ul>
          {#each recent as r (r.path)}
            <li><button type="button" title={r.path} onclick={() => open(r.path)}>{r.name}</button></li>
          {/each}
        </ul>
        <button type="button" onclick={clear}>Clear recent</button>
      </section>
    {/if}
  {/snippet}
  {#snippet trace()}
    <h2>Trace panel</h2>
    <p class="muted">Select an element to see its links.</p>
  {/snippet}
  {#snippet problems()}
    <h2>Problems</h2>
    <p class="muted">Problems appear here once a project is open.</p>
  {/snippet}
</Layout>
<CommandPalette bind:open={paletteOpen} />
<!-- @end -->

<style>
  /* @begin CT-USE-005 */
  /* Colour tokens. Light is the default; dark applies when the OS asks for it and the user
     has not chosen, or when the user chooses it (data-theme is set by applyTheme). Text pairs
     meet WCAG 2.1 AA contrast (CT-USE-004). */
  :global(:root) {
    --bg: #f6f8f9;
    --surface: #ffffff;
    --fg: #1d2a33;
    --muted: #4a5a66;
    --border: #c3ced6;
    --focus: #0b5cad;
    --selected-bg: #cfe3f5;
    --selected-fg: #0f2233;
    --danger: #a12622;
    --backdrop: rgba(10, 20, 30, 0.45);
  }
  :global(:root[data-theme='dark']) {
    --bg: #12181d;
    --surface: #1a2229;
    --fg: #e6edf2;
    --muted: #a9b7c2;
    --border: #34424d;
    --focus: #6cb6ff;
    --selected-bg: #1f4e79;
    --selected-fg: #ffffff;
    --danger: #ff8a84;
    --backdrop: rgba(0, 0, 0, 0.6);
  }
  @media (prefers-color-scheme: dark) {
    :global(:root:not([data-theme])) {
      --bg: #12181d;
      --surface: #1a2229;
      --fg: #e6edf2;
      --muted: #a9b7c2;
      --border: #34424d;
      --focus: #6cb6ff;
      --selected-bg: #1f4e79;
      --selected-fg: #ffffff;
      --danger: #ff8a84;
      --backdrop: rgba(0, 0, 0, 0.6);
    }
  }
  /* @end */

  /* @begin CT-PLT-001, CT-USE-004 */
  :global(body) {
    margin: 0;
    font-family: system-ui, sans-serif;
    color: var(--fg);
    background: var(--bg);
  }
  :global(button),
  :global(select) {
    font: inherit;
    color: var(--fg);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 0.3rem;
    padding: 0.25rem 0.6rem;
  }
  :global(:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  h1 {
    margin: 0 0 0.5rem;
    font-size: 1.3rem;
  }
  h2 {
    margin: 0.25rem 0 0.5rem;
    font-size: 1rem;
  }
  p {
    margin: 0.5rem 0 0;
  }
  .muted {
    color: var(--muted);
  }
  .project {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--muted);
  }
  ul {
    margin: 0.5rem 0;
    padding: 0;
    list-style: none;
  }
  .problem {
    color: var(--danger);
  }
  /* @end */
</style>
