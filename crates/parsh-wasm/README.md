# parsh-wasm

Browser bindings for the shared parsh REPL. Parsing, evaluation, editing,
Tab completion, history, paste handling, and layout are shared with native parsh, with the
`native` feature disabled. An injected `ExecutionHost` provides WASI commands
and temporary files inside the website's module Worker.

A persistent `Session` accepts `input`, `paste`, `resize`, and `reset`, returning
ordered terminal events including raw stdout/stderr byte chunks. An activity
callback lets the page interrupt or time out execution by replacing the Worker.
Reset preserves the host capability and recreates its initial filesystem.

Paredit is always enabled. Structural `input` keys are `sexp-forward`,
`sexp-backward`, `sexp-up`, `sexp-down`, `kill-sexp`, `wrap-sexp`, `splice-sexp`,
`slurp-forward`, `barf-forward`, `slurp-backward`, and `barf-backward`.
The `text` key types characters with automatic pairing; `paste` inserts source
without pairing or executing it.

From the repository root:

```console
rustup target add wasm32-unknown-unknown wasm32-wasip1
pnpm wasm:build
cargo clippy -p parsh-wasm --target wasm32-unknown-unknown -- -D warnings
cargo clippy -p parsh-coreutils --target wasm32-wasip1 -- -D warnings
```
