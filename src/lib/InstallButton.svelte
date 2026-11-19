<script lang="ts">
  import { formatBytes } from './format';
  import { tools } from './tools.svelte';

  // The setup tag prints the size on its stub instead.
  let { showSize = true }: { showSize?: boolean } = $props();
</script>

<button
  class="primary"
  onclick={() => tools.install()}
  disabled={!tools.canChange}
  title={tools.canChange || tools.installing ? undefined : 'Wait for the downloads, the lookup and the Tools to finish'}
>
  {tools.installing ? 'Installing…' : showSize && tools.size ? `Install Tools · ${formatBytes(tools.size)}` : 'Install Tools'}
</button>
