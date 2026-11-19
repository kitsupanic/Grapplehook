import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type ToolState = { kind: 'managed'; path: string; version: string } | { kind: 'system'; path: string } | { kind: 'missing' };

export interface ToolStatus {
  tool: 'ytDlp' | 'ffmpeg' | 'ffprobe' | 'aria2c';
  required: boolean;
  state: ToolState;
}

export interface ToolReport {
  tools: ToolStatus[];
  /** Every Required Tool is Managed or System, so downloads can start. */
  requiredPresent: boolean;
}

export const checkTools = () => invoke<ToolReport>('check_tools');

/** Total bytes Install Tools will download (0 when no Tool is missing), or null if a host does not say. */
export const installSize = (missing: ToolStatus['tool'][]) => invoke<number | null>('install_size', { missing });

export interface InstallProgress {
  tool: ToolStatus['tool'];
  downloaded: number;
  total: number | null;
}

export const installTools = (onProgress: (progress: InstallProgress) => void) => {
  const channel = new Channel<InstallProgress>();
  channel.onmessage = onProgress;
  return invoke<void>('install_tools', { onProgress: channel });
};

/** Removes every Managed Tool. System Tools are never touched. */
export const deleteTools = () => invoke<void>('delete_tools');

export interface UpdateInfo {
  updateAvailable: boolean;
  currentVersion: string;
  latestVersion: string | null;
}

export const checkUpdate = () => invoke<UpdateInfo>('check_update');

export const RELEASES_URL = 'https://github.com/kitsupanic/Grapplehook/releases/latest';

export interface VideoInfo {
  id: string;
  title: string;
  uploader: string | null;
  durationSeconds: number | null;
  thumbnail: string | null;
  /** Distinct video heights, highest first. */
  heights: number[];
}

export const getInfo = (url: string) => invoke<VideoInfo>('get_info', { url });

export interface YtDlpUpdate {
  before: string;
  after: string;
}

export const updateYtDlp = () => invoke<YtDlpUpdate>('update_yt_dlp');

export interface DownloadOptions {
  url: string;
  outputDir: string;
  /** The quality picker's value: `best`, a maximum height such as `1080`, `audio` (audio only) or `muxed` (one combined stream, lower resolution). */
  quality?: string;
  /** Output file name without extension; defaults to the video title. */
  filename?: string;
  /** Convert to .mp4 after downloading; H.264/AAC streams are copied, others re-encoded. */
  toMp4?: boolean;
  /** libx264 quality for a re-encode, 0 to 51 (lower is better). Default 18. */
  crf?: number;
  /** libx264 speed/size trade-off for a re-encode. Default `medium`. */
  preset?: string;
}

/** Starts a download and returns its task id. Progress and the end arrive as events carrying that id. */
export const startDownload = (options: DownloadOptions) => invoke<number>('start_download', { options });

export const cancelDownload = (taskId: number) => invoke<void>('cancel_download', { taskId });

/** The native folder picker; null if the user closes it. */
export const chooseDir = () => invoke<string | null>('choose_dir');

export const readClipboard = () => invoke<string>('read_clipboard');

export type DownloadStage = 'info' | 'download' | 'transcode' | 'done';

export interface DownloadProgress {
  taskId: number;
  stage: DownloadStage;
  /** 0 to 100, or null when unknown. */
  percent: number | null;
  /** Bytes per second. */
  speed: number | null;
  /** Seconds remaining. */
  eta: number | null;
  downloadedBytes: number | null;
  totalBytes: number | null;
}

/** Why a download failed, as far as the fix differs: another quality, another link, or another try. */
export type FailureKind = 'formatUnavailable' | 'videoUnavailable' | 'conversionFailed' | 'other';

/** How a download ended. */
export type DownloadDone = { taskId: number } & (
  | { outcome: 'done'; outputPath: string }
  | { outcome: 'cancelled' }
  | { outcome: 'failed'; kind: FailureKind; error: string; outputPath: string | null }
);

export const onDownloadProgress = (handler: (progress: DownloadProgress) => void) =>
  listen<DownloadProgress>('download-progress', (event) => handler(event.payload));

export const onDownloadDone = (handler: (done: DownloadDone) => void) => listen<DownloadDone>('download-done', (event) => handler(event.payload));
