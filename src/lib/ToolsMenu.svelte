<script lang="ts">
  import InstallButton from './InstallButton.svelte';
  import InstallProgress from './InstallProgress.svelte';
  import UpdateYtDlp from './UpdateYtDlp.svelte';
  import { TOOL_NAMES, tools } from './tools.svelte';

  const label = $derived(
    tools.loadError
      ? 'Tools unavailable'
      : tools.statuses === null
        ? 'Checking Tools…'
        : tools.requiredPresent
          ? 'Tools ready'
          : `${tools.missingRequired} Tool${tools.missingRequired === 1 ? '' : 's'} missing`,
  );
  const tone = $derived(tools.statuses === null && !tools.loadError ? '' : tools.requiredPresent ? 'ok' : 'bad');
  const ytDlp = $derived(tools.statuses?.find((t) => t.tool === 'ytDlp'));
  const anyManaged = $derived(tools.statuses?.some((t) => t.state.kind === 'managed'));
  // Delete asks once more before it removes anything.
  let confirming = $state(false);
  const aria2cMissing = $derived(tools.statuses?.some((t) => t.tool === 'aria2c' && t.state.kind === 'missing'));
</script>

<button class="pill {tone}" popovertarget="tools-sheet" aria-haspopup="dialog">
  <span class="dot {tone}" aria-hidden="true"></span>{label}
</button>

<div id="tools-sheet" class="tools-sheet" popover="auto" role="dialog" aria-label="Tools">
  {#if tools.loadError}
    <p class="error">{tools.loadError}</p>
  {:else if tools.statuses}
    <ul>
      {#each tools.statuses as { tool, required, state } (tool)}
        <li>
          <strong
            >{TOOL_NAMES[tool]}{#if !required}<span class="tag-chip">optional</span>{/if}</strong
          >
          <span class="path" title={state.kind === 'missing' ? undefined : state.path}>
            {state.kind === 'missing' ? 'Not found' : state.path}
          </span>
          {#if state.kind === 'managed'}
            <span class="badge ok" title="Installed by Grapplehook">Managed · {state.version}</span>
          {:else if state.kind === 'system'}
            <span class="badge ok">System</span>
          {:else}
            <span class="badge {required ? 'bad' : ''}">Missing</span>
          {/if}
        </li>
      {/each}
    </ul>
    {#if aria2cMissing}
      <p class="fine">Downloads are slower without aria2c.</p>
    {/if}
    {#if ytDlp}
      <UpdateYtDlp toolState={ytDlp.state} />
    {/if}
    <!-- An unknown size (null) still has something to install; only 0 means nothing is missing. -->
    {#if tools.size !== 0 || anyManaged}
      <div class="task-actions">
        {#if tools.size !== 0}<InstallButton />{/if}
        {#if anyManaged}
          {#if confirming}
            <button
              class="danger"
              disabled={!tools.canChange}
              onclick={async () => {
                confirming = false;
                await tools.remove();
              }}>Delete Managed Tools</button
            >
            <button onclick={() => (confirming = false)}>Keep them</button>
          {:else}
            <button
              disabled={!tools.canChange}
              title={tools.canChange ? 'Remove the Tools Grapplehook installed' : 'Wait for the downloads, the lookup and the Tools to finish'}
              onclick={() => (confirming = true)}>Delete Tools</button
            >
          {/if}
        {/if}
      </div>
      {#if confirming}
        <p class="fine">Grapplehook removes only the Tools it installed. You can install them again later.</p>
      {/if}
    {/if}
    <InstallProgress />
  {/if}
</div>
