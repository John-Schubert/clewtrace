<script lang="ts">
  // @begin CT-PLT-001, CT-EFF-004
  // The app window: shows the app name and version, read from the Rust side, and the
  // project opened from the command line once its background load ends.
  import { getName, getVersion } from '@tauri-apps/api/app';
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
</script>

<!-- @begin CT-PLT-001, CT-EFF-004 -->
<main>
  <h1>{name}</h1>
  {#if version}
    <p>Version {version}</p>
  {/if}
  {#if project}
    <p class="project">{project}</p>
  {/if}
  <p><button type="button" onclick={choose}>Open project folder...</button></p>
  {#if problem}
    <p class="problem" role="alert">{problem}</p>
  {/if}
  {#if recent.length > 0}
    <section aria-label="Recent projects">
      <h2>Recent projects</h2>
      <ul>
        {#each recent as r (r.path)}
          <li><button type="button" title={r.path} onclick={() => open(r.path)}>{r.name}</button></li>
        {/each}
      </ul>
      <button type="button" onclick={clear}>Clear recent</button>
    </section>
  {/if}
</main>
<!-- @end -->

<style>
  /* @begin CT-PLT-001 */
  :global(body) {
    margin: 0;
    font-family: system-ui, sans-serif;
    color: #1d2a33;
    background: #f6f8f9;
  }
  main {
    display: grid;
    place-content: center;
    min-height: 100vh;
    text-align: center;
  }
  h1 {
    margin: 0;
    font-size: 2rem;
  }
  p {
    margin: 0.5rem 0 0;
    color: #4a5a66;
  }
  .project {
    max-width: 40rem;
  }
  /* @end */

  /* @begin CT-SEC-003, CT-USE-007 */
  h2 {
    margin: 1.5rem 0 0;
    font-size: 1rem;
  }
  ul {
    margin: 0.5rem 0;
    padding: 0;
    list-style: none;
  }
  .problem {
    max-width: 40rem;
    color: #a12622;
  }
  /* @end */
</style>
