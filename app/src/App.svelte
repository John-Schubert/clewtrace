<script lang="ts">
  // @begin CT-PLT-001, CT-EFF-004, CT-MOD-005, CT-TRC-005, CT-TRC-004, CT-EFF-006, CT-MOD-004, CT-USE-008, CT-MOD-003
  // The app window: shows the app name and version, read from the Rust side, and the
  // project opened from the command line once its background load ends.
  import { getName, getVersion } from '@tauri-apps/api/app';
  import { CloseGuard, MAIN_WINDOW, windowCloseRequests } from './lib/autosave';
  import {
    ClewError,
    clearRecent,
    describeStatus,
    type DiskNews,
    elementDetail,
    type EditReply,
    type ProjectSummary,
    type ProjectStatus,
    openProject,
    pickFolder,
    recentProjects,
    revertSource,
    waitForDiskChange,
    waitForProject,
    type RecentProject,
  } from './lib/api';
  import { decideDisk, DiskWatch } from './lib/disk';
  import { ActionRegistry } from './lib/actions';
  import type { GapFilter } from './lib/api';
  import { GAP_FILTERS } from './lib/find';
  import CoverageView from './lib/CoverageView.svelte';
  import DetailView from './lib/DetailView.svelte';
  import TextEditor from './lib/TextEditor.svelte';
  import { canEdit, decideOpen, supportsEditor } from './lib/editor';
  import ModelBrowser from './lib/ModelBrowser.svelte';
  import SearchView from './lib/SearchView.svelte';
  import SourceView from './lib/SourceView.svelte';
  import { NavHistory } from './lib/history';
  import { Refresher, stillThere } from './lib/live';
  import TracePanel from './lib/TracePanel.svelte';
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

  // What the main view shows: the detail of the selection, the coverage lists, search, or
  // a source file. The coverage and search views stay up while an element is selected from
  // them, so a long list can be worked through with the trace panel beside it.
  type View = 'detail' | 'coverage' | 'search' | 'source' | 'editor';
  let view = $state<View>('detail');
  let gapFilter = $state<GapFilter>('no-parent');
  let gapCounts = $state<Partial<Record<GapFilter, number>>>();
  let query = $state('');
  let source = $state<{ path: string; line: number }>();

  // The editor for .sysml files. It stays mounted while a file is open, so a switch to
  // another view never drops unsaved edits. Opening a different file saves the edits first;
  // when that save fails, the open waits in `blocked` until the user chooses.
  let editing = $state<{ path: string; line: number }>();
  let editorDirty = $state(false);
  let blocked = $state<{ path: string; line: number }>();
  let editor = $state<ReturnType<typeof TextEditor>>();

  function openSource(path: string, line: number) {
    if (canEdit(path) && supportsEditor()) {
      void openEditor(path, line);
    } else {
      source = { path, line };
      view = 'source';
    }
  }

  async function openEditor(path: string, line: number) {
    view = 'editor';
    if (decideOpen(editing?.path, editorDirty, path) === 'ask' && !(await editor?.save())) {
      blocked = { path, line };
      return;
    }
    blocked = undefined;
    if (editing?.path === path) {
      editor?.goto(line);
    }
    editing = { path, line };
  }

  // @end

  // @begin CT-PLT-001, CT-EFF-004, CT-MOD-005, CT-TRC-005, CT-TRC-004, CT-EFF-006, CT-MOD-004, CT-USE-008, CT-MOD-003
  // Live validation. Each time the Rust side rebuilds the project from the editor's unsaved
  // text, the browser, problems, detail, and trace panes fetch again, and the status line
  // and coverage counts follow. Selection and history keep their element IDs, which are
  // stable across edits. When an edit renames or removes the selected element, the selection
  // is cleared with a short note (ADR-0005), before the panes fetch.
  const refresher = new Refresher();
  let note = $state('');

  async function validated(reply: EditReply) {
    await refreshed(reply.summary);
  }

  // The project was rebuilt, by an edit or by file events; the panes fetch again.
  async function refreshed(summary: ProjectSummary) {
    project = describeStatus({ state: 'ready', project: summary });
    gapCounts = summary.gaps;
    try {
      if (selection?.kind === 'element' && !(await stillThere(selection.id, elementDetail))) {
        selection = undefined;
        note = 'The selected element was renamed or removed';
      }
    } catch (e) {
      show(e);
    }
    refresher.fire();
  }

  async function saveAndOpen() {
    if (blocked && (await editor?.save())) {
      void openEditor(blocked.path, blocked.line);
    }
  }

  // Discarding edits takes the saved text of the file again, so the project matches the disk.
  async function discardAndOpen() {
    if (blocked && editing) {
      const next = blocked;
      const discarded = editing.path;
      editorDirty = false;
      void openEditor(next.path, next.line);
      try {
        validated(await revertSource(discarded));
      } catch (e) {
        show(e);
      }
    }
  }

  function showElement(id: number) {
    view = 'detail';
    visit(id);
  }

  // Back and forward through the elements looked at. The flags mirror the history for the
  // trace panel's buttons.
  const history = new NavHistory();
  let canBack = $state(false);
  let canForward = $state(false);

  function syncHistory() {
    canBack = history.canBack;
    canForward = history.canForward;
  }

  function goto(id: number | undefined) {
    if (id !== undefined) {
      selection = { kind: 'element', id };
      note = '';
    }
    syncHistory();
  }

  // Back and Forward skip the elements an edit removed or renamed.
  async function move(direction: 'back' | 'forward') {
    try {
      goto(await history.step(direction, (id) => stillThere(id, elementDetail)));
    } catch (e) {
      show(e);
    }
  }

  function visit(id: number) {
    history.visit(id);
    goto(id);
  }

  function showStatus(status: ProjectStatus) {
    project = describeStatus(status);
    selection = undefined;
    note = '';
    view = 'detail';
    source = undefined;
    editing = undefined;
    editorDirty = false;
    blocked = undefined;
    query = '';
    gapCounts = status.state === 'ready' ? status.project.gaps : undefined;
    history.clear();
    syncHistory();
    diskConflict = undefined;
    if (status.state === 'ready') {
      opened += 1;
      void diskWatch.start();
    }
  }

  Promise.all([getName(), getVersion()]).then(([n, v]) => {
    name = n;
    version = v;
  });
  waitForProject().then(showStatus);
  // @end

  // @begin CT-EFF-008, CT-USE-009, CT-USE-008, CT-EFF-007
  // File watching. The Rust side watches the project folder and re-reads what changed on
  // disk (a git pull, another editor); one long wait at a time brings the news, never a
  // poll. A clean editor takes the new text; one with unsaved edits stops saving and asks
  // which text to keep. Windows is not watched until task 2.10c.
  let diskConflict = $state<string>();
  const diskWatch = new DiskWatch(waitForDiskChange, heardFromDisk, (e) => show(e));

  async function heardFromDisk(news: DiskNews) {
    if (news.problems.length) {
      show(new ClewError(news.problems[0]));
    }
    const decision = decideDisk(editing?.path, editorDirty, news);
    if (decision === 'reload') {
      await editor?.reloadFromDisk();
    } else if (decision === 'ask' && editing) {
      editor?.hold();
      diskConflict = editing.path;
    }
    if (news.summary) {
      await refreshed(news.summary);
    }
  }

  // Drops the unsaved edits and takes the file as it is on disk.
  async function takeDiskVersion() {
    const path = diskConflict;
    diskConflict = undefined;
    editor?.release(false);
    if (path) {
      try {
        await validated(await revertSource(path));
      } catch (e) {
        show(e);
      }
      await editor?.reloadFromDisk();
    }
  }

  // Keeps the edits; autosave then saves them over the disk version.
  function keepMyEdits() {
    diskConflict = undefined;
    editor?.release(true);
  }
  // @end


  // @begin CT-SEC-003, CT-USE-007, CT-USE-008
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
    // The edits in the open file are saved before another project replaces it.
    if (editor && !(await editor.save())) {
      problem = 'The edits could not be saved, so the other project was not opened. See the editor.';
      return;
    }
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

  // @begin CT-USE-008, CT-USE-007, CT-EFF-002
  // Edits are saved before the window closes. If that save fails, the window stays open
  // once to say so, and closing it again closes it. The listener is set up on the first
  // edit, so an idle app holds none.
  const closeGuard = new CloseGuard(
    () => windowCloseRequests(MAIN_WINDOW),
    () => editor?.save() ?? Promise.resolve(true),
    () => {
      view = 'editor';
      problem = 'The edits could not be saved. Save again, or close the window again to close without them.';
    },
    show,
  );
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

  // @begin CT-USE-002, CT-USE-004, CT-TRC-005, CT-TRC-004, CT-EFF-006
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
  registry.register({ id: 'view.coverage', title: 'Show coverage', run: () => { view = 'coverage'; } });
  for (const f of GAP_FILTERS) {
    registry.register({ id: `coverage.${f.id}`, title: `Coverage: ${f.label.toLowerCase()}`, run: () => { gapFilter = f.id; view = 'coverage'; } });
  }
  registry.register({ id: 'view.search', title: 'Search the model', run: () => { view = 'search'; } });
  registry.register({ id: 'trace.back', title: 'Trace: back', run: () => void move('back') });
  registry.register({ id: 'trace.forward', title: 'Trace: forward', run: () => void move('forward') });
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

<!-- @begin CT-PLT-001, CT-EFF-004, CT-SEC-003, CT-USE-007, CT-USE-004, CT-MOD-005, CT-TRC-005, CT-TRC-004, CT-EFF-006, CT-MOD-004, CT-USE-008, CT-USE-009 -->
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
        <ModelBrowser onselect={showElement} {refresher} />
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
    {#if opened > 0}
      <div class="views" role="group" aria-label="Main views">
        <button type="button" aria-pressed={view === 'detail'} onclick={() => (view = 'detail')}>Detail</button>
        <button type="button" aria-pressed={view === 'coverage'} onclick={() => (view = 'coverage')}>Coverage</button>
        <button type="button" aria-pressed={view === 'search'} onclick={() => (view = 'search')}>Search</button>
        {#if source}
          <button type="button" aria-pressed={view === 'source'} onclick={() => (view = 'source')}>Source</button>
        {/if}
        {#if editing}
          <button type="button" aria-pressed={view === 'editor'} onclick={() => (view = 'editor')}>
            Editor{editorDirty ? ' (unsaved)' : ''}
          </button>
        {/if}
      </div>
      {#if blocked && editing}
        <div class="ask" role="alertdialog" aria-label="Unsaved changes">
          <p>{editing.path} has changes that could not be saved. Opening {blocked.path} would lose them.</p>
          <button type="button" onclick={saveAndOpen}>Save and open {blocked.path}</button>
          <button type="button" onclick={discardAndOpen}>Discard and open {blocked.path}</button>
          <button type="button" onclick={() => (blocked = undefined)}>Keep editing</button>
        </div>
      {/if}
      {#if diskConflict}
        <div class="ask" role="alertdialog" aria-label="Changed on disk">
          <p>{diskConflict} changed on disk while it has unsaved edits here. Nothing is saved until you choose.</p>
          <button type="button" onclick={takeDiskVersion}>Load the version on disk</button>
          <button type="button" onclick={keepMyEdits}>Keep my edits</button>
        </div>
      {/if}
      {#if editing}
        <div class="editor-pane" hidden={view !== 'editor'}>
          <TextEditor bind:this={editor} path={editing.path} line={editing.line} ondirty={(d) => { editorDirty = d; if (d) closeGuard.arm(); }} onvalidated={validated} />
        </div>
      {/if}
      {#key opened}
        {#if view === 'coverage'}
          <CoverageView filter={gapFilter} counts={gapCounts} onfilter={(f) => (gapFilter = f)} onselect={visit} />
        {:else if view === 'search'}
          <SearchView {query} onquery={(q) => (query = q)} onselect={visit} onopen={openSource} />
        {:else if view === 'source' && source}
          <SourceView path={source.path} line={source.line} />
        {/if}
      {/key}
    {/if}
    {#if note && !selection}
      <p class="note" role="status">{note}</p>
    {/if}
    {#if selection && (view === 'detail' || opened === 0)}
      <DetailView {selection} onopen={openSource} {refresher} />
    {/if}
  </main>
  <aside id="pane-trace" tabindex="-1" aria-label="Trace panel">
    <h2>Trace panel</h2>
    {#if opened > 0}
      <TracePanel
        id={selection?.kind === 'element' ? selection.id : undefined}
        {canBack}
        {canForward}
        onfollow={visit}
        onback={() => void move('back')}
        onforward={() => void move('forward')}
        onopen={openSource}
        {refresher}
      />
    {/if}
  </aside>
  <section id="pane-problems" tabindex="-1" aria-label="Problems">
    <h2>Problems</h2>
    {#if opened > 0}
      {#key opened}
        <ProblemsPanel {refresher} onselect={(p) => { view = 'detail'; selection = { kind: 'problem', problem: p }; }} />
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
  .note {
    color: var(--text-muted);
  }
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
  .views {
    display: flex;
    gap: 0.25rem;
    margin: 0.5rem 0;
  }
  .editor-pane {
    height: calc(100% - 6rem);
  }
  .editor-pane[hidden] {
    display: none;
  }
  .ask {
    max-width: 40rem;
    margin: 0.5rem 0;
    padding: 0.25rem 0.75rem 0.5rem;
    border: 1px solid var(--danger);
  }
  .views button[aria-pressed='true'] {
    background: var(--row-selected);
  }
  .problem {
    max-width: 40rem;
    color: var(--danger);
  }
  /* @end */
</style>
