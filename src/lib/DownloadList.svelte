<script lang="ts">
  import DownloadRow from './DownloadRow.svelte';
  import { downloads } from './downloads.svelte';

  $effect(() => downloads.listen());
</script>

{#if downloads.rows.length > 0}
  <section class="downloads" aria-labelledby="downloads-title">
    <h2 id="downloads-title" class="label">Downloads · {downloads.rows.length}</h2>
    <!-- Rows are newest first and never removed, so the claim number is the position counted from the oldest. -->
    {#each downloads.rows as row, i (row.id)}
      <DownloadRow {row} claim={downloads.rows.length - i} />
    {/each}
  </section>
{/if}
