import { expect, test } from 'vitest';
import { parse } from './sentLink';

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
