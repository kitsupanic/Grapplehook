<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type { Snippet } from 'svelte';

  let { href, children }: { href: string; children: Snippet } = $props();
</script>

<a
  {href}
  onclick={(e) => {
    e.preventDefault();
    void openUrl(href);
  }}>{@render children()}</a
>
