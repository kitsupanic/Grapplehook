/** What a sent `grapplehook://open?url=…&quality=…` link asks the app to look up (ADR 0003). */
export interface SentLink {
  url: string;
  quality: string;
}

/**
 * Any web page can open a `grapplehook://` link, so every field is untrusted: only `open` links
 * with an http(s) `url` pass, and a `quality` that isn't a picker value (best, a height, audio,
 * muxed) becomes `best`. It says nothing about whether the video has that height.
 * Unknown fields are ignored, as old Extensions stay installed for a long time.
 */
export function parse(raw: string): SentLink | null {
  let link: URL, url: URL;
  try {
    link = new URL(raw);
    url = new URL(link.searchParams.get('url') ?? '');
  } catch {
    return null;
  }
  // `grapplehook://open/?url=…` is the same link with a trailing slash.
  if (link.protocol !== 'grapplehook:' || link.host !== 'open' || !['', '/'].includes(link.pathname)) return null;
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
  const quality = link.searchParams.get('quality') ?? '';
  return { url: url.href, quality: /^(audio|muxed|[1-9]\d*)$/.test(quality) ? quality : 'best' };
}
