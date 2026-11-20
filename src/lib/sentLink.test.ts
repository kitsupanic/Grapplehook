import { expect, test } from 'vitest';
import { parse, resolveQuality } from './sentLink';

const video = 'https://www.youtube.com/watch?v=abc&t=10';
const link = (query: string) => `grapplehook://open?${query}`;

test('a sent link gives its http(s) URL, decoded', () => {
  expect(parse(link(`url=${encodeURIComponent(video)}`))).toEqual({ url: video, quality: 'best' });
  expect(parse(link('url=http%3A%2F%2Fa.example%2Fv'))).toEqual({ url: 'http://a.example/v', quality: 'best' });
});

test('a link with any other scheme, path or no url is rejected', () => {
  expect(parse(link('url=javascript%3Aalert(1)'))).toBeNull();
  expect(parse(link('url=file%3A%2F%2F%2Fetc%2Fpasswd'))).toBeNull();
  expect(parse(link('url=grapplehook%3A%2F%2Fopen%3Furl%3Dhttps%253A%252F%252Fa.example'))).toBeNull();
  expect(parse(link('url=not%20a%20url'))).toBeNull();
  expect(parse(link('url='))).toBeNull();
  expect(parse(link('quality=720'))).toBeNull();
  expect(parse(`grapplehook://download?url=${encodeURIComponent(video)}`)).toBeNull();
  expect(parse(`https://evil.example/open?url=${encodeURIComponent(video)}`)).toBeNull();
  expect(parse('garbage')).toBeNull();
  expect(parse('')).toBeNull();
});

test('a link to this machine or the local network is rejected', () => {
  for (const host of [
    'localhost',
    'app.localhost',
    'localhost.',
    'printer.local',
    '127.0.0.1',
    '2130706433',
    '0.0.0.0',
    '10.1.2.3',
    '172.16.0.1',
    '172.31.255.255',
    '192.168.1.1',
    '169.254.1.1',
    '[::1]',
    '[fd00::1]',
    'router',
    'fritz.box',
    'nas.lan',
    'pi.home.arpa',
    'svc.internal',
    '100.64.0.1',
    '100.127.255.255',
    '0.1.2.3',
  ]) {
    expect(parse(link(`url=${encodeURIComponent(`http://${host}/reboot`)}`)), host).toBeNull();
  }
  // Public neighbours of the private ranges still pass.
  for (const host of ['172.15.0.1', '172.32.0.1', '11.0.0.1', '100.63.0.1', '100.128.0.1', 'localhost.example.com', 'lan.example.com']) {
    expect(parse(link(`url=${encodeURIComponent(`http://${host}/v`)}`)), host).not.toBeNull();
  }
});

test('quality is kept when the picker knows it, and best otherwise', () => {
  const url = `url=${encodeURIComponent(video)}`;
  expect(parse(link(`${url}&quality=720`))?.quality).toBe('720');
  expect(parse(link(`${url}&quality=audio`))?.quality).toBe('audio');
  expect(parse(link(`${url}&quality=muxed`))?.quality).toBe('muxed');
  expect(parse(link(`${url}&quality=8k-hdr`))?.quality).toBe('best');
  expect(parse(link(`${url}&quality=0`))?.quality).toBe('best');
});

test('fields the app does not know are ignored', () => {
  expect(parse(link(`url=${encodeURIComponent(video)}&from=extension&v=2`))).toEqual({ url: video, quality: 'best' });
});

test('a sent height becomes the highest available height at or below it', () => {
  expect(resolveQuality('1080', [2160, 1080, 720])).toBe('1080');
  expect(resolveQuality('1080', [720, 480])).toBe('720');
  expect(resolveQuality('1000', [1440, 720, 360])).toBe('720');
});

test('a sent height with nothing at or below it is best', () => {
  expect(resolveQuality('360', [1080, 720])).toBe('best');
  expect(resolveQuality('1080', [])).toBe('best');
});

test('best, audio and muxed pass through', () => {
  expect(resolveQuality('best', [1080])).toBe('best');
  expect(resolveQuality('audio', [1080])).toBe('audio');
  expect(resolveQuality('muxed', [])).toBe('muxed');
});
