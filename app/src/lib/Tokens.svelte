<!-- @begin CT-USE-005, CT-USE-004 -->
<script lang="ts"></script>
<!-- Color tokens for both themes. Text on background pairs meet WCAG 2.1 AA contrast (4.5:1),
     and focus rings meet 3:1. The OS setting applies until the user picks a theme, which sets
     data-theme on the root element. A style-only component, since the trace scanner reads
     Svelte but not plain CSS. -->
<style>
  :global(:root) {
    color-scheme: light;
    --bg: #f6f8f9;
    --surface: #ffffff;
    --border: #c5d0d8;
    --text: #1d2a33;
    --text-muted: #44545f;
    --accent: #0b5cad;
    --accent-text: #ffffff;
    --danger: #a12622;
    --focus: #0b5cad;
    --row-hover: #e6eef3;
    --row-selected: #d3e4f4;
  }
  :global(:root[data-theme='dark']) {
    color-scheme: dark;
    --bg: #11181d;
    --surface: #1a242b;
    --border: #3a4a55;
    --text: #e4ecf1;
    --text-muted: #a9b9c4;
    --accent: #6cb2f0;
    --accent-text: #0b1620;
    --danger: #ff8f8a;
    --focus: #8cc4f5;
    --row-hover: #243541;
    --row-selected: #2c4559;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root:not([data-theme])) {
      color-scheme: dark;
      --bg: #11181d;
      --surface: #1a242b;
      --border: #3a4a55;
      --text: #e4ecf1;
      --text-muted: #a9b9c4;
      --accent: #6cb2f0;
      --accent-text: #0b1620;
      --danger: #ff8f8a;
      --focus: #8cc4f5;
      --row-hover: #243541;
      --row-selected: #2c4559;
    }
  }
</style>
<!-- @end -->
