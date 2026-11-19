<script lang="ts">
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { downloads, type Download } from './downloads.svelte';
  import { claimNo, formatBytes, formatDuration } from './format';

  let { row, claim }: { row: Download; claim: number } = $props();

  const STAGES = { info: 'Preparing…', download: 'Downloading', transcode: 'Converting to .mp4', done: 'Done' };
  // The routing strip: one segment per stage, filled up to the current one.
  const ROUTE = [
    ['info', 'Prepare'],
    ['download', 'Download'],
    ['transcode', 'Convert'],
    ['done', 'Saved'],
  ] as const;

  const running = $derived(row.state === 'running');
  const current = $derived(row.state === 'done' ? ROUTE.length : ROUTE.findIndex(([s]) => s === row.stage));
  const stage = $derived(
    row.state === 'cancelled' ? 'Cancelled' : row.state === 'failed' ? 'Failed' : row.state === 'done' ? 'Done' : STAGES[row.stage],
  );
  const stats = $derived.by(() => {
    if (row.state === 'done') return row.outputPath ?? '';
    if (row.state === 'failed') return row.error || 'Unknown error';
    if (row.state === 'cancelled') return '';
    return [
      row.percent != null && `${row.percent.toFixed(0)}%`,
      row.downloadedBytes != null && row.totalBytes != null && `${formatBytes(row.downloadedBytes)} / ${formatBytes(row.totalBytes)}`,
      row.speed != null && `${formatBytes(row.speed)}/s`,
      row.eta != null && `ETA ${formatDuration(row.eta)}`,
    ]
      .filter(Boolean)
      .join('  ·  ');
  });
  // A video that is gone needs a different fix than a dropped connection or a full disk.
  const RECOVERY = {
    formatUnavailable: 'Pick another quality, then download again.',
    videoUnavailable: 'Check the link and that the video is public, then fetch it again.',
    conversionFailed: 'The video was saved without converting. Most players can open it as it is.',
    other: 'Try the download again. If it keeps failing, check your connection and free disk space.',
  };
  const recovery = $derived(RECOVERY[row.errorKind ?? 'other']);
  let revealError = $state('');

  async function reveal() {
    revealError = '';
    try {
      await revealItemInDir(row.outputPath!);
    } catch (err) {
      revealError = String(err);
    }
  }
</script>

<article class="dl-tag {row.state}">
  <div class="dl-body">
    <div class="task-head">
      <span class="task-title" title={row.title}>{row.title}</span>
      <span class="task-stage">{stage}</span>
    </div>
    <div
      class="route"
      role="progressbar"
      aria-label={`Progress of ${row.title}`}
      aria-valuenow={row.percent === null ? undefined : Math.round(row.percent)}
      aria-valuemin="0"
      aria-valuemax="100"
    >
      {#each ROUTE as [key, label], i (key)}
        <span class="seg" class:full={i < current} class:indeterminate={running && i === current && row.percent === null} title={label}>
          {#if running && i === current}<span class="fill" style:width="{row.percent ?? 0}%"></span>{/if}
        </span>
      {/each}
    </div>
    <span class="task-stats" title={stats}>{stats}</span>
    {#if row.state === 'failed'}
      <p class="fine">
        {recovery}
      </p>
    {/if}
    {#if revealError}
      <p class="error" role="alert">{revealError}</p>
    {/if}
  </div>
  <div class="dl-stub">
    <span class="claim-no" aria-label={`Download ${claim}`}>{claimNo(claim)}</span>
    {#if row.outputPath}
      <button class="stub-btn" onclick={reveal}>Show in folder</button>
    {/if}
    {#if running}
      <button disabled={row.cancelling} onclick={() => downloads.cancel(row.id)}>{row.cancelling ? 'Cancelling…' : 'Cancel'}</button>
    {/if}
  </div>
</article>
