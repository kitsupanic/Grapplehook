import { expect, test, vi } from 'vitest';
import type { ToolReport } from './api';

const api = vi.hoisted(() => ({
  checkTools: vi.fn(),
  installSize: vi.fn(),
}));

vi.mock('./api', () => ({
  ...api,
  cancelDownload: vi.fn(),
  onDownloadProgress: async () => () => {},
  onDownloadDone: async () => () => {},
}));

const { tools } = await import('./tools.svelte');

const report: ToolReport = {
  tools: [{ tool: 'ffmpeg', required: true, state: { kind: 'missing' } }],
  requiredPresent: false,
};

test('a slow size from an older refresh never replaces the newer one', async () => {
  api.checkTools.mockResolvedValue(report);
  const sizes: ((bytes: number) => void)[] = [];
  api.installSize.mockImplementation(() => new Promise<number>((resolve) => sizes.push(resolve)));

  const older = tools.refresh();
  await vi.waitFor(() => expect(sizes).toHaveLength(1));
  const newer = tools.refresh();
  await vi.waitFor(() => expect(sizes).toHaveLength(2));

  // The newer answer comes first; a stale 0 would hide Install Tools while ffmpeg is missing.
  sizes[1](5000);
  await newer;
  sizes[0](0);
  await older;

  expect(tools.size).toBe(5000);
});

test('no Tool Change is offered while a lookup runs, since core would refuse it', () => {
  expect(tools.canChange).toBe(true);
  tools.lookingUp = true;
  expect(tools.canChange).toBe(false);
  tools.lookingUp = false;
  expect(tools.canChange).toBe(true);
});

test('no Tool Change is offered while a Tools check runs, and an older check never replaces a newer one', async () => {
  const found: ToolReport = { tools: [{ tool: 'ffmpeg', required: true, state: { kind: 'system', path: '/bin/ffmpeg' } }], requiredPresent: true };
  const checks: ((r: ToolReport) => void)[] = [];
  api.checkTools.mockImplementation(() => new Promise<ToolReport>((resolve) => checks.push(resolve)));
  api.installSize.mockResolvedValue(0);

  const older = tools.refresh();
  const newer = tools.refresh();
  expect(tools.canChange).toBe(false);

  checks[1](found);
  await newer;
  expect(tools.canChange).toBe(false);
  checks[0](report);
  await older;

  expect(tools.canChange).toBe(true);
  expect(tools.requiredPresent).toBe(true);
  expect(tools.size).toBe(0);
});
