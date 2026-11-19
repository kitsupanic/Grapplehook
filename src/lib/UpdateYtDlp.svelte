<script lang="ts">
  import type { ToolState } from './api';
  import { tools } from './tools.svelte';

  let { toolState }: { toolState: ToolState } = $props();

  let message = $state('');
  let error = $state('');

  async function update() {
    message = error = '';
    try {
      const { before, after } = await tools.updateYtDlp();
      message = before === after ? `yt-dlp is already up to date (${after})` : `yt-dlp updated: ${before} → ${after}`;
    } catch (err) {
      error = String(err);
    }
  }
</script>

<div class="task-actions">
  <button onclick={update} disabled={toolState.kind !== 'managed' || !tools.canChange}>
    {tools.updating ? 'Updating…' : 'Update yt-dlp'}
  </button>
</div>
{#if toolState.kind === 'system'}
  <p class="fine">Grapplehook never changes Tools you installed yourself. Update yt-dlp the way you installed it.</p>
{:else if toolState.kind === 'missing'}
  <p class="fine">Install yt-dlp first.</p>
{/if}
{#if message}
  <p class="fine" role="status">{message}</p>
{/if}
{#if error}
  <p class="error">{error}</p>
{/if}
