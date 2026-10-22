---
title: "Getting started"
description: "Build and run parsh on Linux."
---

Native parsh requires Linux and a current stable Rust toolchain. Build from the repository:

```console
git clone https://github.com/jmjoy/parsh.git
cd parsh
cargo build --release
./target/release/parsh
```

To install it on your PATH, run `cargo install --path crates/parsh`.

## Ways to run

```console
parsh
parsh -c '(echo hello)'
parsh examples/hello.parsh world
```

With no arguments, parsh opens an interactive editor when stdin is a terminal,
otherwise it reads a script from stdin. `parsh -` explicitly reads stdin.
`--help` and `--version` are also available. Script arguments are the list
`$args`; it excludes the script filename. Scripts can start with
`#!/usr/bin/env parsh` once the binary is on PATH.

Options are parsed by clap. After the script path or `-c SOURCE`, every remaining
argument is passed through unchanged, including `--help` and `--`.
Use `parsh -- -script.parsh` for a script filename beginning with `-`.

## Your first session

Type `str "hello, " "parsh")` at the opening-parenthesis prompt. parsh displays `"hello, parsh"`. Enter is for multi-line input; a closing outer parenthesis evaluates the expression.

```parsh
(let name world)
(str "hello, " $name)
(echo hello $name)
```

The first expression binds a value. The second returns text, which an interactive session displays. The third runs the native `echo` program and writes stdout. Script mode does not print expression return values automatically.

The [browser playground](/parsh/en/playground/) supports language expressions and coreutils commands. See its [capabilities](/parsh/en/docs/playground/) for the command list and temporary files.
