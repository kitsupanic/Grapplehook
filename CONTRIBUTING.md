# Contributing

Thanks for helping. Bug reports, fixes and small improvements are all welcome. For a bigger change, please open an issue first so we can agree on the approach before you write the code.

By taking part you agree to the [code of conduct](CODE_OF_CONDUCT.md).

## Setup

You need [Node.js](https://nodejs.org) 22+, [pnpm](https://pnpm.io), a [Rust toolchain](https://rustup.rs) and the [Tauri system prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```sh
pnpm install
pnpm tauri dev
```

## Before you open a pull request

CI runs these on every pull request, so run them locally first:

```sh
pnpm lint && pnpm typecheck && pnpm build
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

`pnpm format` fixes most formatting problems.

The tests in `src-tauri/tests/real_*.rs` are ignored by default because they use the network and download real Tools. Run them by hand if you touch downloading or Tool installs; the comment at the top of each file shows how.

## How the code is laid out

- `src-tauri/src/core/`: all the logic. Its public interface is the test seam, and tests live in `src-tauri/tests/`. New behaviour needs a test there.
- `src-tauri/src/commands.rs`: a thin Tauri command layer over core.
- `src/`: the Svelte frontend. It only calls commands and renders the result.

Use the words defined in [GLOSSARY.md](GLOSSARY.md) (Tool, Managed Tool, System Tool, ...) in code, UI text and issues. Past decisions are recorded in [docs/adr/](docs/adr/); if your change goes against one, say so in the pull request. UI work should follow [DESIGN.md](DESIGN.md).

## Pull requests

- Keep each pull request to one change, and explain what it does and why.
- For a UI change, add a screenshot.
- By contributing, you agree that your work is licensed under the [MIT License](LICENSE).

## Security issues

Please don't report them in public issues. See [SECURITY.md](SECURITY.md).
