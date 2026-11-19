<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { checkUpdate, RELEASES_URL } from './api';
</script>

<!-- Any failure counts as "no update": the notice just stays hidden. -->
{#await checkUpdate().catch(() => null) then info}
  {#if info?.updateAvailable}
    <button class="pill update" title="Open the releases page" onclick={() => openUrl(RELEASES_URL)}>
      v{info.latestVersion} available
    </button>
  {/if}
{/await}
