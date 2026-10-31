<script lang="ts">
  // @begin CT-PLT-001, CT-EFF-004
  // The app window: shows the app name and version, read from the Rust side, and the
  // project opened from the command line once its background load ends.
  import { getName, getVersion } from '@tauri-apps/api/app';
  import { describeStatus, waitForProject } from './lib/api';

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
</style>
