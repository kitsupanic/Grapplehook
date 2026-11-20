# Glossary

**Tool**
An external program Grapplehook runs to do its work: `yt-dlp`, `ffmpeg`, `ffprobe` or `aria2c`.
_Avoid_: module, dependency, binary.

**Managed Tool**
A Tool that Grapplehook downloaded itself, after the user agreed, into its own data folder. Grapplehook can update it. When both copies exist, Grapplehook prefers it over a System Tool.

**System Tool**
A Tool the user installed themselves, found on their `PATH`. Grapplehook uses it but never changes it.
_Avoid_: PATH tool, external tool.

**Required Tool**
A Tool without which nothing can be downloaded: `yt-dlp`, `ffmpeg` and `ffprobe`.

**Optional Tool**
A Tool that only makes things faster: `aria2c`. Without it, downloads still work, just more slowly.

**Tool Run**
Grapplehook running a Tool to do work: a download or a video lookup. Many Tool Runs can be in flight at once.

**Tool Change**
Installing, updating or deleting Managed Tools. A Tool Change needs the Tools to itself: it is refused while any Tool Run is in flight, and no Tool Run can start while it is in progress.

**Extension**
The Grapplehook browser extension. It sends the link of a YouTube video, with a chosen quality, to Grapplehook. It never downloads anything itself.
_Avoid_: plugin, add-on.
