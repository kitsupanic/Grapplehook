## What and why

<!-- One change per pull request. What does it do, and why? Link the issue: "Closes #123". -->

## How it was checked

<!-- Commands you ran, platforms you tried. For a UI change, add a screenshot. -->

- [ ] `pnpm lint && pnpm typecheck && pnpm build`
- [ ] `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- [ ] New behaviour has a test in `src-tauri/tests/`
- [ ] If this goes against a decision in `docs/adr/`, I say so above

See [CONTRIBUTING.md](../CONTRIBUTING.md) for setup and layout.
