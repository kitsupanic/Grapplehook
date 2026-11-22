<script lang="ts">
  import { formatBytes } from './format';
  import { heldLink } from './heldLink.svelte';
  import InstallButton from './InstallButton.svelte';
  import InstallProgress from './InstallProgress.svelte';
  import { TOOL_NAMES, tools } from './tools.svelte';

  const PURPOSE = {
    ytDlp: 'downloads the video',
    ffmpeg: 'merges and converts',
    ffprobe: 'reads the video format',
    aria2c: 'faster downloads',
  };
</script>

<!-- First-run setup: shown in place of the URL row while a Required Tool is Missing. -->
<section class="tag setup" aria-labelledby="setup-title">
  <div class="tag-body">
    <h2 id="setup-title">Grapplehook needs a few Tools first</h2>
    {#if heldLink.waiting}
      <!-- A sent link (ADR 0003) waits in heldLink and is looked up once the URL row comes back. -->
      <p class="fine held-link" role="status">
        Got a link from your browser: <span class="held-url">{heldLink.waiting}</span>. It opens once the Tools are installed.
      </p>
    {/if}
    <ul class="setup-list">
      {#each tools.statuses ?? [] as { tool, required, state } (tool)}
        <li>
          <span class="tool-name"
            >{TOOL_NAMES[tool]}{#if !required}<span class="tag-chip">optional</span>{/if}</span
          >
          <span class="tool-purpose">{PURPOSE[tool]}</span>
          <span class="tool-state" class:missing={state.kind === 'missing' && required}>
            {#if state.kind !== 'missing'}Found{:else if required}Missing{:else}Not found{/if}
          </span>
        </li>
      {/each}
    </ul>
    <InstallProgress />
    <p class="fine">Downloaded from each Tool's official releases over https and checked against the checksum each release publishes.</p>
  </div>
  <div class="stub">
    {#if tools.size}<span class="label">{formatBytes(tools.size)}</span>{/if}
    <InstallButton showSize={false} />
    <!-- For Tools installed outside the app (e.g. with Homebrew) while it is open. -->
    <button disabled={tools.changing} onclick={() => tools.refresh()}>Check again</button>
  </div>
</section>
