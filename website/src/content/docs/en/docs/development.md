---
title: "Development"
description: "Test native parsh, build WASM, and develop the website."
---

```console
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Tests include real subprocesses, large pipelines, file redirection, lexical
functions, and Linux PTYs for paste, history, Unicode editing, foreground
interrupts, terminal restoration, and separation of queued source from child
stdin.

## Website and WASM

Use Node.js 24+, pnpm 12.4.2, a current stable Rust toolchain, and wasm-pack 0.13.1.
Run the following commands from the repository root:

```console
rustup target add wasm32-unknown-unknown wasm32-wasip1
cargo install wasm-pack --version 0.13.1 --locked
pnpm install --frozen-lockfile
pnpm dev
```

Open the local URL with `/parsh/en/` or `/parsh/zh-cn/`. The dev command builds WASM first. After editing Rust, rerun `pnpm wasm:build` and reload the page.

```console
pnpm check
pnpm build
pnpm --filter parsh-website exec playwright install chromium
pnpm test
```

Browser tests run against the production output; start `pnpm preview --port 4321` first. For managed local preview, run `agproc start website`. After edits, run `agproc restart website`.

Generated WASM, dependencies, build output, and browser test artifacts are ignored by Git. GitHub Actions runs the same checks on pull requests and deploys successful builds on `master` or manual dispatch. Set the repository's Pages source to **GitHub Actions** for the first deployment.

The browser crate disables the `parsh` library's default `native` feature. Parsing, evaluation, editing, history, and layout are shared with Linux. An injectable host runs WASI coreutils inside the Worker and supplies a temporary filesystem. `parsh-coreutils` pins the uutils revision; its first build downloads upstream dependencies.
