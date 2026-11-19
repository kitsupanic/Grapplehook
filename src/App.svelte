<script lang="ts">
  import DownloadList from './lib/DownloadList.svelte';
  import ToolStatus from './lib/ToolStatus.svelte';
  import Topbar from './lib/Topbar.svelte';
  import UrlInfo from './lib/UrlInfo.svelte';
  import { tools } from './lib/tools.svelte';

  tools.refresh();
</script>

<Topbar />

<main>
  {#if tools.statuses && !tools.requiredPresent}
    <ToolStatus />
  {:else}
    <UrlInfo />
  {/if}
  <DownloadList />
</main>
