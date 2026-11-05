# parsh website

English | [简体中文](README.zh-CN.md)

The [website](https://jmjoy.github.io/parsh/en/) includes a command playground and
[documentation](https://jmjoy.github.io/parsh/en/docs/).
[简体中文](https://jmjoy.github.io/parsh/zh-cn/) is also available.

The playground runs the Rust interpreter and 29 real uutils coreutils commands as
WebAssembly, with temporary files, capture, pipelines, and redirection. Functions,
macros, collections, JSON, bytes, complete results, and exceptions share the native
implementation. HTTP requests and background tasks require native parsh. The
[examples guide](https://jmjoy.github.io/parsh/en/docs/examples/) identifies scripts
that can run in the browser.

## Development

Use Node.js 24+, pnpm 12.4.2, stable Rust, wasm-pack 0.13.1, and agproc.
Run from the repository root:

```console
rustup target add wasm32-unknown-unknown wasm32-wasip1
cargo install wasm-pack --version 0.13.1 --locked
pnpm install --frozen-lockfile
agproc skills
agproc start website
```

Open `/parsh/en/` or `/parsh/zh-cn/` at `http://127.0.0.1:4321`.
agproc builds WASM and the static site and manages its preview. After edits, run
`agproc restart website`; stop it with `agproc stop website` when finished.

```console
pnpm check
pnpm --filter parsh-website exec playwright install chromium
pnpm test
```

Browser tests use the existing production build and reuse a running local preview;
Playwright's configuration starts a preview if none is running. `pnpm build`
separately builds WASM and the static site; `pnpm wasm:build` builds only WASM
assets. See the [development guide](src/content/docs/en/docs/development.md) for
all checks and target-specific requirements.

## Deployment

GitHub Actions checks pull requests and deploys successful builds from `master`
or manual dispatch to Pages. For the first deployment, set
**Settings → Pages → Source** to **GitHub Actions**.
