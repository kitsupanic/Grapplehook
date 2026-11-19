# Security policy

## Supported versions

Only the [latest release](https://github.com/kitsupanic/Grapplehook/releases/latest) gets security fixes.

## Reporting a vulnerability

Please do not open a public issue. Report it privately through [GitHub's private vulnerability reporting](https://github.com/kitsupanic/Grapplehook/security/advisories/new) instead.

Include what you found, how to reproduce it, and which version and OS you used. You should get a reply within a week.

## Scope

In scope: Grapplehook itself, including how it downloads, verifies and runs Managed Tools, and how it passes URLs and options to the Tools.

Out of scope: bugs in yt-dlp, ffmpeg, ffprobe or aria2c themselves. Report those to their own projects (links are in the [README](README.md#third-party-tools)). The release builds are not code-signed; that is known and documented.
