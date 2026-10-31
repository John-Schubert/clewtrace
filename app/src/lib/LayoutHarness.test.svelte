<script lang="ts">
  // @begin CT-USE-004
  // Test harness: mounts Layout with a short text in each pane.
  import Layout from './Layout.svelte';
  // @end
</script>

<!-- @begin CT-USE-004 -->
<Layout>
  {#snippet header()}Title{/snippet}
  {#snippet browser()}Browser{/snippet}
  {#snippet main()}Main{/snippet}
  {#snippet trace()}Trace{/snippet}
  {#snippet problems()}Problems{/snippet}
</Layout>
<!-- @end -->
