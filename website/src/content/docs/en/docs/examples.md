---
title: "Example scripts"
description: "Run the repository examples by topic and check native and browser support."
---

The repository’s [`examples/`](https://github.com/jmjoy/parsh/tree/master/examples) directory contains complete, runnable scripts.
Run them from the repository root:

```console
cargo run -- examples/hello.parsh world
cargo run -- examples/stdlib.parsh
cargo run -- examples/execution.parsh
```

Scripts print explicitly with `echo`, `printf`, and `dump`; file mode does not automatically display return values.
`dump` uses the REPL display format: `(dump [#1 #true])` prints `(#1 #true)`.

## Basic syntax

| Script | Demonstrates | Environment |
| --- | --- | --- |
| [hello.parsh](https://github.com/jmjoy/parsh/blob/master/examples/hello.parsh) | Script arguments, named functions, and command conditions | Native only |
| [variables.parsh](https://github.com/jmjoy/parsh/blob/master/examples/variables.parsh) | Bindings, interpolation, raw strings, lists, get, and export | Native and browser |
| [literals.parsh](https://github.com/jmjoy/parsh/blob/master/examples/literals.parsh) | Typed # literals and explicit int/float/bool conversion | Native and browser |
| [arithmetic.parsh](https://github.com/jmjoy/parsh/blob/master/examples/arithmetic.parsh) | Arithmetic, numeric comparisons, and text equality | Native and browser |

## Functions and control flow

| Script | Demonstrates | Environment |
| --- | --- | --- |
| [functions.parsh](https://github.com/jmjoy/parsh/blob/master/examples/functions.parsh) | Recursion, lexical scope, closures, optional/rest parameters, return, and apply | Native and browser |
| [for.parsh](https://github.com/jmjoy/parsh/blob/master/examples/for.parsh) | List iteration, fresh scopes, continue, break, and nested loops | Native and browser |
| [while.parsh](https://github.com/jmjoy/parsh/blob/master/examples/while.parsh) | Conditional iteration, outer bindings, and loop return values | Native and browser |
| [structs.parsh](https://github.com/jmjoy/parsh/blob/master/examples/structs.parsh) | Constructors, accessors, predicates, nesting, and value equality | Native and browser |
| [macros.parsh](https://github.com/jmjoy/parsh/blob/master/examples/macros.parsh) | Quoted forms, quasiquote, splicing, optional/rest parameters, and caller scope | Native and browser |

## Collections and data

| Script | Demonstrates | Environment |
| --- | --- | --- |
| [stdlib.parsh](https://github.com/jmjoy/parsh/blob/master/examples/stdlib.parsh) | Immutable lists, Unicode text, higher-order transforms, and numeric helpers | Native and browser |
| [maps.parsh](https://github.com/jmjoy/parsh/blob/master/examples/maps.parsh) | Immutable updates, eager defaults, ordered keys, merge, and entries | Native and browser |
| [json.parsh](https://github.com/jmjoy/parsh/blob/master/examples/json.parsh) | Struct encoding, typed decoding, byte input, and JSON errors | Native and browser |
| [bytes.parsh](https://github.com/jmjoy/parsh/blob/master/examples/bytes.parsh) | Binary values, UTF-8, slicing, concatenation, Base64, and hex | Native and browser |

## Commands and execution

| Script | Demonstrates | Environment |
| --- | --- | --- |
| [pipelines.parsh](https://github.com/jmjoy/parsh/blob/master/examples/pipelines.parsh) | Pipelines, captured text, and numeric conversion at command boundaries | Native and browser |
| [files.parsh](https://github.com/jmjoy/parsh/blob/master/examples/files.parsh) | Temporary directories, glob, redirection, and finally cleanup | Native only |
| [execution.parsh](https://github.com/jmjoy/parsh/blob/master/examples/execution.parsh) | Command references, complete results, $?, raw capture, check, and exceptions | Native and browser |
| [tasks.parsh](https://github.com/jmjoy/parsh/blob/master/examples/tasks.parsh) | Background execution, isolated state, wait, job-status, and cancellation | Native only |
| [http.parsh](https://github.com/jmjoy/parsh/blob/master/examples/http.parsh) | Native HTTP responses, headers, byte bodies, and JSON decoding | Native only |

## Running in the browser

Paste scripts marked “Native and browser” into the [playground](/parsh/en/playground/) and press Enter.
Use a fresh session for each script to avoid duplicate top-level `let` bindings. The browser has no
script-file entry point, `$args` is empty, and pipelines buffer one stage at a time; native pipelines stream.
`hello.parsh` depends on script arguments and repository paths, `files.parsh` uses `mktemp`,
`tasks.parsh` uses native background tasks, and `http.parsh` sends native HTTP requests.
See [playground capabilities](/parsh/en/docs/playground/) for the command list and limits.

## Files, tasks, and HTTP

`files.parsh` creates a directory with `mktemp -d` and uses `finally` to restore the working directory
and remove its files, including after a runtime error in its body. `tasks.parsh` starts and immediately
cancels a `sleep 30`; you do not need to wait the full 30 seconds.

The HTTP example sends a request only when given a URL:

```console
cargo run -- examples/http.parsh http://127.0.0.1:8080/data
```

Replace the URL with a reachable service. The script prints response information and decodes
JSON or UTF-8 text according to `Content-Type`. HTTP 4xx/5xx statuses remain data;
connection and other transport failures throw errors.

Read [variables and scope](/parsh/en/docs/variables/), the [standard library](/parsh/en/docs/stdlib/),
[functions and control flow](/parsh/en/docs/control-flow/), and [execution results and tasks](/parsh/en/docs/execution/)
for the language rules behind these examples.
