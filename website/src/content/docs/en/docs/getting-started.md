---
title: "Getting started"
description: "Build and run parsh on Linux."
---

Native parsh requires Linux and a current stable Rust toolchain. Build from the repository:

```console
git clone https://github.com/jmjoy/parsh.git
cd parsh
cargo build --locked --release
./target/release/parsh
```

To install it on your PATH, run `cargo install --path crates/parsh`.

## Ways to run

```console
parsh
parsh -c '(echo hello)'
parsh -c '(printf "%s\n" $args)' first 'two words'
parsh examples/hello.parsh world
parsh - first < examples/hello.parsh
```

With no arguments, parsh opens an interactive editor when stdin is a terminal,
otherwise it reads a script from stdin. `parsh -` explicitly reads stdin.
`--help` and `--version` are also available. Script arguments are the list
`$args`; it excludes the script filename. Scripts can start with
`#!/usr/bin/env parsh` once the binary is on PATH.

Options are parsed by clap. After the script path or `-c SOURCE`, every remaining
argument is passed through unchanged, including `--help` and `--`.
Use `parsh -- -script.parsh` for a script filename beginning with `-`.

## Startup configuration

Interactive parsh runs `config.parsh` before the first prompt. It uses
`$XDG_CONFIG_HOME/parsh/config.parsh` when `XDG_CONFIG_HOME` is absolute, falling
back to `$HOME/.config/parsh/config.parsh` when `HOME` is absolute. Automatic
configuration requires one of those base directories.

When parsh first creates its `parsh` configuration directory, it writes a template
with commented examples and prints its path to stderr. If the directory already
exists, you can create or remove `config.parsh` yourself.

The file runs as ordinary Parsh code. This example sets an environment variable
and defines a command for the session.

```parsh
(export EDITOR vi)
(defn hello () (echo "hello from parsh"))
```

`-c`, script files, and scripts read from stdin skip automatic configuration.
Set `--config FILE` before the command or script to load a specific file, or
start with `--no-config` to skip configuration. A relative `FILE` is resolved
from the startup working directory. The two options are mutually exclusive.

```console
parsh --no-config
parsh --config ./settings.parsh -c '(hello)'
```

See [interactive configuration](/parsh/en/docs/interactive/#configuration) for
how startup scripts are evaluated and how errors are handled.

## Your first session

Type `dump (str "hello, " "parsh")` at the `$ ()` prompt and press Enter. parsh supplies the outer closing parenthesis and displays `"hello, parsh"`. Enter inserts a newline when input is unfinished; Ctrl+J always inserts a newline.

```parsh
(let name world)
(dump (str "hello, " $name))
(echo hello $name)
```

The first expression binds a value. The second explicitly prints text using `dump`. The third runs the native `echo` program and writes stdout. Neither interactive nor script mode automatically prints return values.

The [browser playground](/parsh/en/playground/) supports language expressions and coreutils commands. See its [capabilities](/parsh/en/docs/playground/) for the command list and temporary files.

See [example scripts](/parsh/en/docs/examples/) for complete programs covering collections, functions, command results, and background tasks.
