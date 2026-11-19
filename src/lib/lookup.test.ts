import { expect, test } from 'vitest';
import { downloadUrl } from './lookup';

test('a download fetches the looked-up link, and none once the box holds another', () => {
  expect(downloadUrl('  https://a.example/v  ', 'https://a.example/v')).toBe('https://a.example/v');
  // Edited after Fetch info: the shown title and quality belong to the old link.
  expect(downloadUrl('https://b.example/v', 'https://a.example/v')).toBeNull();
  expect(downloadUrl('', '')).toBeNull();
});
