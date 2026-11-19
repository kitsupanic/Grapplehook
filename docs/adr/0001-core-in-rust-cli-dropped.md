# Core logic lives in the Rust backend; there is no CLI

The core logic lives in Rust as Tauri commands. The Svelte frontend only calls those commands and listens for progress events. There is no CLI, so nothing needs core in TypeScript. Installing Managed Tools (download, checksum, unpack, mark executable) needs Rust anyway. Running Tools from the webview through `tauri-plugin-shell` would give the frontend permission to run `yt-dlp` with arbitrary arguments, and could leave Tool processes running after the window closes.

## Considered Options

- **Keep core in TypeScript in the webview, using `tauri-plugin-shell`.** It meant less Rust, but a wide shell permission and weaker process cleanup.
- **Ship a CLI that shares a Node core.** That meant keeping a Node runtime alongside Rust for a small CLI.

## Consequences

There is no separate CLI or npm package. A CLI can come back later as a mode of the Tauri binary, through `tauri-plugin-cli`.
