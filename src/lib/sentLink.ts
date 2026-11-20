// This machine and the local network: a page must not make yt-dlp send requests to the user's router or printer.
// A name without a dot (`http://router/`) and home-network suffixes (fritz.box, nas.lan) resolve inside it.
// The URL parser has already turned forms like `2130706433` into dotted IPv4. 100.64.0.0/10 is
// carrier-grade NAT and Tailscale. Every IPv6 literal is refused, as video links never use one.
// shortcut: a public name that resolves to a local address still passes; checking would need DNS in core.
const LOCAL_HOST =
  /^([^.]+|.+\.(localhost|local|lan|home\.arpa|internal|box)|0\.[\d.]+|127\.[\d.]+|10\.[\d.]+|192\.168\.[\d.]+|169\.254\.[\d.]+|172\.(1[6-9]|2\d|3[01])\.[\d.]+|100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.[\d.]+|\[.*\])$/;

/** What a sent `grapplehook://open?url=…&quality=…` link asks the app to look up (ADR 0003). */
export interface SentLink {
  url: string;
  quality: string;
}

/**
 * Any web page can open a `grapplehook://` link, so every field is untrusted: only `open` links
 * with an http(s) `url` on the public internet pass, and a `quality` that isn't a picker value (best, a height, audio,
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
  if (LOCAL_HOST.test(url.hostname.replace(/\.$/, ''))) return null;
  const quality = link.searchParams.get('quality') ?? '';
  return { url: url.href, quality: /^(audio|muxed|[1-9]\d*)$/.test(quality) ? quality : 'best' };
}
