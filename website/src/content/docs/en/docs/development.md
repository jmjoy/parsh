---
title: "Development"
description: "Test native and portable parsh, build WASM, and preview the website with agproc."
---

Run the native and portable checks from the repository root:

```console
cargo fmt --all --check
cargo test --locked
cargo test --locked --no-default-features --lib
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --no-default-features --lib -- -D warnings
```

Tests cover subprocesses, large pipelines, redirection, lexical functions,
collections, JSON, local HTTP services, execution results, exceptions, and background
task isolation and cancellation. Linux PTY tests cover paste, history, Unicode
editing, foreground interrupts, terminal restoration, and separation of queued
source from child stdin.

Run the [example scripts](/parsh/en/docs/examples/) when changing a documented
language feature, for example `cargo run -- examples/execution.parsh`.

## Website and WASM

Use Node.js 24+, pnpm 12.4.2, a current stable Rust toolchain, wasm-pack 0.13.1,
and agproc. Install the build prerequisites from the repository root:

```console
rustup target add wasm32-unknown-unknown wasm32-wasip1
cargo install wasm-pack --version 0.13.1 --locked
pnpm install --frozen-lockfile
```

The repository's `agproc.toml` builds the production site and manages its preview
on port 4321. Load the project guide before managing the service:

```console
agproc skills
agproc start website
```

Open `http://127.0.0.1:4321/parsh/en/` or
`http://127.0.0.1:4321/parsh/zh-cn/`. After editing Rust, website code, or docs,
run `agproc restart website` to rebuild WASM and the site and refresh the preview.
`agproc start` leaves an already running service unchanged. Use `agproc ps` to
inspect status and `agproc stop website` when finished.

Check the website and run browser tests against the production output:

```console
pnpm check
pnpm --filter parsh-website exec playwright install chromium
pnpm test
```

Playwright reuses the running preview locally. Its configuration can start a
preview if none is running, including in CI; it does not build the site. Use
`pnpm build` when only a static production build is needed. `pnpm wasm:build`
rebuilds just the browser interpreter and coreutils assets.

The workflow also checks the target-specific Rust crates:

```console
cargo clippy --locked -p parsh-coreutils --target wasm32-wasip1 -- -D warnings
cargo clippy --locked -p parsh-wasm --target wasm32-unknown-unknown -- -D warnings
```

Generated WASM, dependencies, build output, and browser test artifacts are ignored
by Git. GitHub Actions runs these checks on pull requests and deploys successful
builds on `master` or manual dispatch. CI uses a preinstalled Chrome/Chromium when
available and installs Playwright's headless shell otherwise. Set the repository's
Pages source to **GitHub Actions** for the first deployment.

The browser crate disables the `parsh` library's default `native` feature.
Parsing, evaluation, editing, history, and layout are shared with Linux. An
injectable host runs WASI coreutils inside the Worker and supplies a temporary
filesystem. `parsh-coreutils` pins the uutils revision; its first build downloads
upstream dependencies.
