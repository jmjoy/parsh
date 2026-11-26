---
title: "Getting started"
description: "Install, build, and run parsh on Linux."
---

Native parsh runs on Linux and requires a current stable Rust toolchain.

## Install

Install the latest stable release from crates.io:

```console
cargo install parsh
```

To try the current development version from GitHub:

```console
cargo install --git https://github.com/jmjoy/parsh
```

Both commands install `parsh` to Cargo's binary directory. Make sure that
directory is on your `PATH`, then run `parsh` to start the shell.

## Build from source

To build the latest source from the repository:

```console
git clone https://github.com/jmjoy/parsh.git
cd parsh
cargo build --locked --release
./target/release/parsh
```

To install a local checkout on your `PATH`, run `cargo install --path crates/parsh`.

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

## Guides for AI agents

The native binary serves English guides for its version. List the topics and
load the ones relevant to your task.

```console
parsh skills
parsh skills get syntax
parsh skills get commands execution --full
parsh skills get --all --json
```

`parsh skills` defaults to `list`. `get` prints Markdown, and `--full` adds
related example sources where available. `path` prints logical resource IDs
such as `embedded://parsh/skills/syntax`. Use `get` to read the guide.
`list`, `get`, and `path` accept `--json` for structured output.
Use `parsh ./skills` to execute a script file named `skills`.

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

Capturing command output removes surrounding Unicode whitespace by default. Add `&trim #false` after the capture body to preserve the exact text.

```parsh
(dump (<-out (printf " hello\n")))
(dump (<-out (printf " hello\n") &trim #false))
```

These calls display `"hello"` and `" hello\n"`.

The [browser playground](/parsh/en/playground/) supports language expressions and coreutils commands. See its [capabilities](/parsh/en/docs/playground/) for the command list and temporary files.

See [example scripts](/parsh/en/docs/examples/) for complete programs covering collections, functions, command results, and background tasks.
