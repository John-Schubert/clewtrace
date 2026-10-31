<script lang="ts">
  // @begin CT-EFF-005
  // Test harness: mounts VirtualList with a plain text row.
  import VirtualList from './VirtualList.svelte';

  let { count, onrange }: { count: number; onrange: (start: number, end: number) => void } = $props();
  // @end
</script>

<!-- @begin CT-EFF-005 -->
<VirtualList {count} rowHeight={20} label="Test rows" {onrange}>
  {#snippet row(index: number)}Row {index}{/snippet}
</VirtualList>
<!-- @end -->
