# parsh-coreutils

A WASI Preview 1 multicall runner for the website's 29-command uutils subset.
Upstream utility implementations and startup macros are pinned to the same
Git revision in `Cargo.toml`; the workspace lockfile pins their dependencies.
The vendored `pwd` keeps that revision with a small documented WASI adaptation
for Rust's unsupported filesystem canonicalization.

The browser supplies argv with the utility name as argv[0], a fresh WASI
instance per command, an isolated environment, and a shared virtual filesystem.
The `parsh.cwd_len` and `parsh.cwd_write` imports supply the actual UTF-8 working
directory independently of the exported `PWD` variable. The runner calls
`set_current_dir` before dispatching the untouched arguments to uutils.

`pnpm wasm:build` builds both browser WASM modules, derives the advertised
command list from this crate's dependencies, and copies the upstream MIT
license to the static output. The built binaries are not committed.
