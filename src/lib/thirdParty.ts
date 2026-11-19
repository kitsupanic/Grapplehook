export interface ThirdPartyTool {
  name: string;
  license: string;
  sourceLabel: string;
  sourceUrl: string;
}

/** Keep in sync with "Third-party Tools" in README.md. */
export const THIRD_PARTY_TOOLS: ThirdPartyTool[] = [
  { name: 'yt-dlp', license: 'Unlicense', sourceLabel: 'github.com/yt-dlp/yt-dlp', sourceUrl: 'https://github.com/yt-dlp/yt-dlp' },
  { name: 'ffmpeg', license: 'GPL builds', sourceLabel: 'ffmpeg.org', sourceUrl: 'https://ffmpeg.org/download.html' },
  { name: 'ffprobe', license: 'GPL builds', sourceLabel: 'ffmpeg.org', sourceUrl: 'https://ffmpeg.org/download.html' },
  { name: 'aria2c', license: 'GPLv2+', sourceLabel: 'github.com/aria2/aria2', sourceUrl: 'https://github.com/aria2/aria2' },
];
