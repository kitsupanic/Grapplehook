<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app';
  import ExternalLink from './ExternalLink.svelte';
  import { THIRD_PARTY_TOOLS } from './thirdParty';

  let dialog: HTMLDialogElement;
</script>

<button class="pill" onclick={() => dialog.showModal()}>About</button>

<dialog bind:this={dialog} class="about" aria-labelledby="about-title">
  <h2 id="about-title">Grapplehook</h2>
  <p class="fine">
    {#await getVersion() then version}Version {version}{/await} · MIT license ·
    <ExternalLink href="https://github.com/kitsupanic/Grapplehook">Source</ExternalLink>
  </p>

  <h3>Third-party Tools</h3>
  <p class="fine">Grapplehook runs these Tools but does not bundle them.</p>
  <ul>
    {#each THIRD_PARTY_TOOLS as tool (tool.name)}
      <li>
        <strong>{tool.name}</strong>
        <span class="fine">{tool.license}</span>
        <ExternalLink href={tool.sourceUrl}>{tool.sourceLabel}</ExternalLink>
      </li>
    {/each}
  </ul>

  <form method="dialog"><button>Close</button></form>
</dialog>
