<script lang="ts">
  import { TOOL_NAMES, tools } from './tools.svelte';

  // Progress is per download, so the bar restarts for each Tool; the name says which one it is.
  const percent = $derived(tools.progress?.total ? (tools.progress.downloaded / tools.progress.total) * 100 : null);
</script>

{#if tools.installing}
  {#if tools.progress}
    <p class="fine" role="status">Downloading {TOOL_NAMES[tools.progress.tool]}…</p>
  {/if}
  <div
    class="route single"
    role="progressbar"
    aria-label={tools.progress ? `Installing ${TOOL_NAMES[tools.progress.tool]}` : 'Installing Tools'}
    aria-valuenow={percent === null ? undefined : Math.round(percent)}
    aria-valuemin="0"
    aria-valuemax="100"
  >
    <span class="seg" class:indeterminate={percent === null}><span class="fill" style:width="{percent ?? 0}%"></span></span>
  </div>
{/if}
{#if tools.installError}
  <p class="error">{tools.installError}</p>
{/if}
