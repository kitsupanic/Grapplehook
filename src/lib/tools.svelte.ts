import { checkTools, deleteTools, installSize, installTools, updateYtDlp, type InstallProgress, type ToolReport, type ToolStatus } from './api';
import { downloads } from './downloads.svelte';

export const TOOL_NAMES: Record<ToolStatus['tool'], string> = {
  ytDlp: 'yt-dlp',
  ffmpeg: 'ffmpeg',
  ffprobe: 'ffprobe',
  aria2c: 'aria2c',
};

let report = $state<ToolReport | null>(null);
let loadError = $state('');
// Bytes Install Tools will download: 0 when nothing is missing, null while unknown.
let size = $state<number | null>(null);
// Only the newest refresh may set the report and the size: an older check can finish last.
let refreshCall = 0;
// Tools checks in flight. Core counts each as a Tool Run and refuses a Tool Change meanwhile.
let checking = $state(0);
// The Tool Change in progress. Core refuses lookups, downloads and other changes meanwhile.
let change = $state<'install' | 'delete' | 'update' | null>(null);
let progress = $state<InstallProgress | null>(null);
let installError = $state('');
// A video lookup in flight. Core refuses a Tool Change meanwhile, as it does during a download.
let lookingUp = $state(false);

/** Shared Tool status and Install Tools state, read by the topbar Tools sheet and the setup card. */
export const tools = {
  /** `null` until the first check finishes. */
  get statuses() {
    return report?.tools ?? null;
  },
  /** True once every Required Tool is Managed or System, as core reports it; download controls stay disabled until then. */
  get requiredPresent() {
    return report?.requiredPresent ?? false;
  },
  get missingRequired() {
    return report?.tools.filter((t) => t.required && t.state.kind === 'missing').length ?? 0;
  },
  get loadError() {
    return loadError;
  },
  get size() {
    return size;
  },
  /** True while Tools are installed, deleted or updated. */
  get changing() {
    return change !== null;
  },
  get installing() {
    return change === 'install';
  },
  get updating() {
    return change === 'update';
  },
  /** False while a download or a lookup runs or the Tools already change: core would refuse a Tool Change. */
  get canChange() {
    return change === null && !downloads.active && !lookingUp && checking === 0;
  },
  /** Set by Fetch info for as long as its lookup runs. */
  get lookingUp() {
    return lookingUp;
  },
  set lookingUp(value: boolean) {
    lookingUp = value;
  },
  get progress() {
    return progress;
  },
  get installError() {
    return installError;
  },
  async refresh() {
    const call = ++refreshCall;
    checking++;
    let checked: ToolReport | null = null;
    let error = '';
    try {
      checked = await checkTools();
    } catch (err) {
      error = String(err);
    } finally {
      checking--;
    }
    if (call !== refreshCall) return;
    if (checked) report = checked;
    loadError = error;
    // A failed check knows nothing missing for sure: unknown size, so Install stays offered.
    if (loadError || !report) {
      size = null;
      return;
    }
    const missing = report.tools.filter((t) => t.state.kind === 'missing').map((t) => t.tool);
    // Sizing can take seconds per host.
    const bytes = await installSize(missing).catch(() => null);
    if (call === refreshCall) size = bytes;
  },
  async install() {
    if (change) return;
    change = 'install';
    installError = '';
    progress = null;
    try {
      await installTools((p) => (progress = p));
    } catch (err) {
      installError = String(err);
    }
    change = null;
    await tools.refresh();
  },
  /** Delete every Managed Tool, then check again. Errors land in `installError`. */
  async remove() {
    if (change) return;
    change = 'delete';
    installError = '';
    try {
      await deleteTools();
    } catch (err) {
      installError = String(err);
    }
    change = null;
    await tools.refresh();
  },
  /** Run `yt-dlp -U` on the Managed yt-dlp, then check again. Errors are thrown to the caller. */
  async updateYtDlp() {
    if (change) throw new Error('Wait until the Tools finish changing.');
    change = 'update';
    try {
      return await updateYtDlp();
    } finally {
      change = null;
      tools.refresh();
    }
  },
};
