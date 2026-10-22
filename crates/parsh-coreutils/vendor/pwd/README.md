# WASI adaptation of uutils pwd

Copied from uutils/coreutils commit d2d96946baaf19bc6b0434438f0fa59be54d6a0f,
`src/uu/pwd/src/pwd.rs`, under the MIT license included here.

The only behavior change skips filesystem canonicalization for the WASI
physical-path branch: Rust does not implement canonicalize for wasm32-wasip1,
and this playground filesystem does not contain symlinks. The working
directory supplied by the WASI runner is already an absolute physical path.
Argument parsing, help, diagnostics, and command execution remain upstream.
