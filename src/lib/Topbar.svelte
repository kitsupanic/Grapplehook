<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app';
  import About from './About.svelte';
  import HookMark from './HookMark.svelte';
  import ToolsMenu from './ToolsMenu.svelte';
  import UpdateNotice from './UpdateNotice.svelte';
</script>

<header class="topbar">
  <div class="wordmark">
    <HookMark /> grapplehook{#await getVersion() then version}<span class="version">v{version}</span>{/await}
  </div>
  <div class="tools">
    <UpdateNotice />
    <ToolsMenu />
    <About />
  </div>
</header>
