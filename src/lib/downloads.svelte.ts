import { cancelDownload, onDownloadDone, onDownloadProgress, type DownloadDone, type DownloadProgress, type FailureKind } from './api';

export interface Download {
  id: number;
  title: string;
  state: 'running' | 'done' | 'failed' | 'cancelled';
  stage: DownloadProgress['stage'];
  percent: number | null;
  speed: number | null;
  eta: number | null;
  downloadedBytes: number | null;
  totalBytes: number | null;
  /** Cancel was clicked and the Tools are being stopped. */
  cancelling: boolean;
  outputPath: string | null;
  error: string | null;
  errorKind: FailureKind | null;
}

// Newest first, as in the old UI. Rows are never removed.
const rows = $state<Download[]>([]);

// Events can arrive before `add` names the row (start_download returns the id after the task
// has started), so an unknown id gets a row instead of being dropped.
function row(id: number) {
  const existing = rows.find((r) => r.id === id);
  if (existing) return existing;
  rows.unshift({
    id,
    title: '',
    state: 'running',
    stage: 'info',
    percent: null,
    speed: null,
    eta: null,
    downloadedBytes: null,
    totalBytes: null,
    cancelling: false,
    outputPath: null,
    error: null,
    errorKind: null,
  });
  return rows[0];
}

function applyProgress(p: DownloadProgress) {
  const r = row(p.taskId);
  if (r.state !== 'running') return;
  r.stage = p.stage;
  r.percent = p.percent;
  r.speed = p.speed;
  r.eta = p.eta;
  r.downloadedBytes = p.downloadedBytes;
  r.totalBytes = p.totalBytes;
}

function applyDone(d: DownloadDone) {
  const r = row(d.taskId);
  r.state = d.outcome;
  r.cancelling = false;
  if (d.outcome === 'done') {
    r.outputPath = d.outputPath;
    r.percent = 100;
  } else if (d.outcome === 'failed') {
    r.error = d.error;
    r.errorKind = d.kind;
    // A failed conversion still saved the original.
    r.outputPath = d.outputPath;
  }
}

/** Every download of this session, each with its own progress. */
export const downloads = {
  get rows() {
    return rows;
  },
  /** True while any download is running. */
  get active() {
    return rows.some((r) => r.state === 'running');
  },
  /** Names the row of a download that was just started. */
  add(id: number, title: string) {
    row(id).title = title;
  },
  async cancel(id: number) {
    row(id).cancelling = true;
    try {
      await cancelDownload(id);
    } catch {
      row(id).cancelling = false;
    }
  },
  /** Start applying download events to the rows. Returns a function that stops. */
  listen() {
    const stops = Promise.all([onDownloadProgress(applyProgress), onDownloadDone(applyDone)]);
    return () => void stops.then((fns) => fns.forEach((stop) => stop()));
  },
};
