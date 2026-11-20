import { expect, test, vi } from 'vitest';

const plugin = vi.hoisted(() => ({
  getCurrent: vi.fn(async (): Promise<string[] | null> => null),
  onOpenUrl: vi.fn<(handler: (urls: string[]) => void) => Promise<() => void>>(async () => () => {}),
}));
vi.mock('@tauri-apps/plugin-deep-link', () => plugin);

const { heldLink } = await import('./heldLink.svelte');
const send = plugin.onOpenUrl.mock.calls[0][0];
const sent = (url: string) => `grapplehook://open?url=${encodeURIComponent(url)}`;

test('a link waits while the box is locked, and only the newest one is kept', () => {
  send([sent('https://a.example/v')]);
  send([sent('https://b.example/v')]);
  // A rejected link changes nothing: b stays held.
  send(['grapplehook://open?url=javascript%3Aalert(1)', 'garbage']);
  expect(heldLink.take(true)).toBeNull();
  expect(heldLink.take(false)).toBe('https://b.example/v');
  // Handed out once.
  expect(heldLink.take(false)).toBeNull();
});
