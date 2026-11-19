import { expect, test, vi } from 'vitest';
import type { DownloadDone, DownloadProgress } from './api';

// The two event handlers `downloads.listen()` registers, so the test can fire events.
const handlers = vi.hoisted(() => ({
  progress: null as ((p: DownloadProgress) => void) | null,
  done: null as ((d: DownloadDone) => void) | null,
}));

vi.mock('./api', () => ({
  cancelDownload: vi.fn(),
  onDownloadProgress: async (h: (p: DownloadProgress) => void) => ((handlers.progress = h), () => {}),
  onDownloadDone: async (h: (d: DownloadDone) => void) => ((handlers.done = h), () => {}),
}));

const { downloads } = await import('./downloads.svelte');

const progress = (taskId: number, percent: number): DownloadProgress => ({
  taskId,
  stage: 'download',
  percent,
  speed: null,
  eta: null,
  downloadedBytes: null,
  totalBytes: null,
});

test('events that beat add() and progress after the end each land on one row', async () => {
  downloads.listen();
  await Promise.resolve();

  // start_download returns the id only after the task has started, so its events can come first.
  handlers.progress!(progress(1, 40));
  handlers.done!({ taskId: 1, outcome: 'done', outputPath: '/out/a.mp4' });
  downloads.add(1, 'First video');
  // A late progress event must not bring a finished download back to life.
  handlers.progress!(progress(1, 60));

  downloads.add(2, 'Second video');
  handlers.progress!(progress(2, 10));
  handlers.done!({ taskId: 2, outcome: 'failed', kind: 'formatUnavailable', error: 'Requested format is not available', outputPath: null });

  expect(downloads.rows.map((r) => [r.id, r.title, r.state, r.percent])).toEqual([
    [2, 'Second video', 'failed', 10],
    [1, 'First video', 'done', 100],
  ]);
  expect(downloads.rows[0].errorKind).toBe('formatUnavailable');
  expect(downloads.rows[1].outputPath).toBe('/out/a.mp4');
  // A failed conversion still saved the original, so its row keeps the path to show it.
  downloads.add(3, 'Third video');
  handlers.done!({ taskId: 3, outcome: 'failed', kind: 'conversionFailed', error: 'Converting failed', outputPath: '/out/c.webm' });
  expect(downloads.rows[0].outputPath).toBe('/out/c.webm');
  expect(downloads.active).toBe(false);
});
