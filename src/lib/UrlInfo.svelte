<script lang="ts">
  import { untrack } from 'svelte';
  import { downloadDir } from '@tauri-apps/api/path';
  import { chooseDir, getInfo, readClipboard, startDownload, type VideoInfo } from './api';
  import { downloads } from './downloads.svelte';
  import { heldLink } from './heldLink.svelte';
  import { claimNo, formatDuration } from './format';
  import { downloadUrl } from './lookup';
  import { tools } from './tools.svelte';

  let url = $state('');
  const busy = $derived(tools.lookingUp);
  let error = $state('');
  let info = $state<VideoInfo | null>(null);
  let quality = $state('best');
  // The URL the info belongs to: a download uses it, not whatever the input holds by now.
  let fetchedUrl = $state('');
  const target = $derived(downloadUrl(url, fetchedUrl));
  let outDir = $state('');
  let filename = $state('');
  let toMp4 = $state(false);
  let crf = $state(18);
  let preset = $state('medium');
  // Same list as PRESETS in src-tauri/src/core/download.rs, which rejects any other name.
  const PRESETS = ['ultrafast', 'superfast', 'veryfast', 'faster', 'fast', 'medium', 'slow', 'slower', 'veryslow'];
  let starting = $state(false);

  // Start in the downloads folder; nothing is remembered between launches.
  downloadDir()
    .then((dir) => (outDir ||= dir))
    .catch(() => {});

  // A lookup is refused while the Tools change (install, delete, update).
  const locked = $derived(busy || tools.changing || !tools.requiredPresent);
  const lockedTitle = $derived(
    tools.changing ? 'Wait for the Tools to finish changing' : tools.requiredPresent ? undefined : 'Install the Required Tools first',
  );

  async function fetchInfo() {
    if (locked) return;
    tools.lookingUp = true;
    error = '';
    info = null;
    // Read before the await: the box can be edited while the info is on its way.
    const target = url.trim();
    try {
      info = await getInfo(target);
      fetchedUrl = target;
      quality = 'best';
      // A name typed for the last video would otherwise be given to this one.
      filename = '';
    } catch (err) {
      error = String(err);
    } finally {
      tools.lookingUp = false;
    }
  }

  /** Put a link in the box and look it up, as Paste and a sent link do. Never starts a download. */
  function lookUp(link: string) {
    url = link;
    fetchInfo();
  }

  // Paste and fetch in one click: pasting a URL is the whole point of the row.
  async function paste() {
    if (locked) return;
    try {
      lookUp((await readClipboard()).trim());
    } catch (err) {
      error = String(err);
    }
  }

  $effect(() => {
    const link = heldLink.take(locked);
    if (link) untrack(() => lookUp(link));
  });

  async function browse() {
    try {
      outDir = (await chooseDir()) ?? outDir;
    } catch (err) {
      error = String(err);
    }
  }

  async function download() {
    if (starting || !info || !target || tools.changing || !tools.requiredPresent) return;
    if (!outDir) {
      error = 'Pick a folder first.';
      return;
    }
    starting = true;
    error = '';
    // Read before the await: Fetch info can replace `info` while the download starts.
    const title = info.title || fetchedUrl;
    try {
      const id = await startDownload({
        url: target,
        outputDir: outDir,
        quality,
        filename: filename.trim() || undefined,
        toMp4,
        // An emptied box is null and a typed fraction or negative is no u32, which core rejects with a raw serde error.
        crf: Math.max(0, Math.round(crf ?? 18)),
        preset,
      });
      downloads.add(id, title);
    } catch (err) {
      error = String(err);
    } finally {
      starting = false;
    }
  }

  const folderName = $derived(outDir.split(/[\\/]/).filter(Boolean).pop() ?? outDir);
  const subtitle = (i: VideoInfo) => [i.uploader, i.durationSeconds != null && formatDuration(i.durationSeconds)].filter(Boolean).join(' · ');
</script>

<section class="fetch-row" aria-label="Video URL">
  <input
    id="url"
    type="url"
    placeholder="Paste a video URL…"
    spellcheck="false"
    bind:value={url}
    onkeydown={(e) => e.key === 'Enter' && fetchInfo()}
  />
  <button disabled={locked} onclick={paste} title="Paste a URL from the clipboard and fetch its info">Paste</button>
  <!-- Orange belongs to the live item: once the tag is out, Fetch info steps back until the URL changes. -->
  <button class:primary={!info || !target} disabled={locked} title={lockedTitle} onclick={fetchInfo}>{busy ? 'Fetching…' : 'Fetch info'}</button>
</section>

{#if error}
  <p class="error" role="alert">{error}</p>
{/if}

{#if !info && !busy}
  <section class="tag blank" aria-label="How to start">
    <div class="tag-body">
      <h2>No video yet</h2>
      <p>Copy a video link in your browser, then press Paste.</p>
    </div>
    <div class="stub" aria-hidden="true"><span class="label">Claim no.</span><span class="claim-no">{claimNo(downloads.rows.length + 1)}</span></div>
  </section>
{/if}

{#if info}
  <section class="tag" aria-label="Video info">
    <div class="tag-body">
      <div class="info-head">
        {#if info.thumbnail}
          <img class="thumb" src={info.thumbnail} alt="" />
        {/if}
        <div class="info-meta">
          <h2>{info.title || info.id}</h2>
          <p class="fine">{subtitle(info)}</p>
        </div>
      </div>
      <div class="options-clip">
        <div class="options">
          <label class="opt">
            <span class="label">Quality</span>
            <select bind:value={quality}>
              <option value="best">Best available</option>
              {#each info.heights as h (h)}
                <option value={String(h)}>{h}p</option>
              {/each}
              <option value="audio">Audio only</option>
              <option value="muxed">Muxed (video + audio in one file)</option>
            </select>
          </label>
          <div class="opt">
            <span class="label" id="save-to">Save to</span>
            <button type="button" class="seg-value" aria-labelledby="save-to" aria-describedby="save-to-path" title={outDir} onclick={browse}
              >{folderName || 'Pick a folder'} <span class="change">Change</span></button
            >
            <span id="save-to-path" hidden>{outDir}</span>
          </div>
          <label class="opt">
            <span class="label">Filename</span>
            <input bind:value={filename} placeholder="Video title" spellcheck="false" />
          </label>
          <label class="opt check">
            <input type="checkbox" bind:checked={toMp4} disabled={quality === 'audio'} />
            Convert to .mp4
          </label>
          {#if toMp4 && quality !== 'audio'}
            <label class="opt">
              <span class="label">Quality (crf, lower is better)</span>
              <input type="number" min="0" max="51" step="1" bind:value={crf} />
            </label>
            <label class="opt">
              <span class="label">Encoding speed</span>
              <select bind:value={preset}>
                {#each PRESETS as p (p)}
                  <option value={p}>{p}</option>
                {/each}
              </select>
            </label>
          {/if}
        </div>
      </div>
    </div>
    <div class="stub">
      <span class="label">Claim no.</span>
      <span class="claim-no">{claimNo(downloads.rows.length + 1)}</span>
      <button class="stub-btn" disabled={starting || tools.changing || !tools.requiredPresent || !target} title={lockedTitle} onclick={download}
        >{starting ? 'Starting…' : 'Download'}</button
      >
    </div>
  </section>
{/if}
