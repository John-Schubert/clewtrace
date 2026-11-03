<!-- @begin CT-EFF-005 -->
<script lang="ts">
  // Test harness: a VirtualList of numbered rows, since the list takes a row snippet.
  import VirtualList from './VirtualList.svelte';
  let { count, hinted = false }: { count: number; hinted?: boolean } = $props();
</script>

<div style="height: 480px">
  <VirtualList {count} label="Numbers" rowAt={(i) => i} hint={hinted ? (v) => `file-${v}.sysml:${v + 1}` : undefined}>
    {#snippet row(value: number)}Row {value}{/snippet}
  </VirtualList>
</div>
<!-- @end -->
