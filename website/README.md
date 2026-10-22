# parsh website

English | [简体中文](README.zh-CN.md)

The [website](https://jmjoy.github.io/parsh/en/) includes a command playground and
[documentation](https://jmjoy.github.io/parsh/en/docs/).
[简体中文](https://jmjoy.github.io/parsh/zh-cn/) is also available.

The playground runs the Rust interpreter and 29 real uutils coreutils commands
as WebAssembly, with temporary files, command capture, pipes, and redirections.

## Development

Website development requires Node.js 24+, pnpm 12.4.2, and wasm-pack 0.13.1.
Run these commands from the repository root:

```console
rustup target add wasm32-unknown-unknown wasm32-wasip1
cargo install wasm-pack --version 0.13.1 --locked
pnpm install --frozen-lockfile
pnpm dev
```

Open `/parsh/en/` or `/parsh/zh-cn/` on the printed local URL.
`pnpm build` builds WASM and the static site.
`pnpm check` checks types; `pnpm test` tests the
production build after `pnpm --filter parsh-website exec playwright install chromium`.

## Deployment

GitHub Actions checks pull requests and deploys successful builds from `master`
to Pages. For the first deployment, set **Settings → Pages → Source** to
**GitHub Actions**.
