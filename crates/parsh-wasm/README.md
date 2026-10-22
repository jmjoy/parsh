# parsh-wasm

Browser bindings for parsh's isolated syntax REPL. This crate uses the shared
reader, evaluator, input editor, history, paste handling, and layout without the
`native` feature. It has no filesystem or process backend.

The website runs a persistent `Session` in a module Worker. `input`, `paste`,
`resize`, and `reset` return ordered terminal events; an activity callback marks
expression evaluation so the page can interrupt or time out the Worker.

From the repository root:

```console
pnpm wasm:build
cargo clippy -p parsh-wasm --target wasm32-unknown-unknown -- -D warnings
```
