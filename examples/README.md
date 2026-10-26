# parsh examples

English | [简体中文](README.zh-CN.md)

Run the scripts from the repository root with native parsh:

```console
cargo run -- examples/hello.parsh world
cargo run -- examples/while.parsh
cargo run -- examples/files.parsh
cargo run -- examples/structs.parsh
cargo run -- examples/macros.parsh
```

Once `parsh` is on PATH, scripts can also run directly, for example
`./examples/while.parsh`. Each script includes its run command and prints its
results explicitly.

| Script | Demonstrates |
| --- | --- |
| [hello.parsh](hello.parsh) | Script arguments, functions, and a command condition |
| [variables.parsh](variables.parsh) | Bindings, interpolation, literal strings, lists, `get`, and exported environment values |
| [arithmetic.parsh](arithmetic.parsh) | Accumulation, arithmetic, numeric comparisons, and text equality |
| [literals.parsh](literals.parsh) | Typed `#` literals, maps, and explicit int/float/bool conversions |
| [stdlib.parsh](stdlib.parsh) | Immutable lists, text, numeric helpers, closures, optional/rest parameters, return, and macros |
| [functions.parsh](functions.parsh) | Recursion, lexical scope, typed return values, and function output capture |
| [for.parsh](for.parsh) | List iteration, `continue`, `break`, outer bindings, and nested loops |
| [while.parsh](while.parsh) | Conditional loops, `continue`, a `break` value, and the last iteration's return value |
| [pipelines.parsh](pipelines.parsh) | Streaming pipelines, captured output, and arithmetic on command output |
| [files.parsh](files.parsh) | Working directories, globbing, file redirection, handled command failures, and cleanup |
| [structs.parsh](structs.parsh) | Struct definitions, constructors, accessors, predicates, nesting, and value equality |
| [macros.parsh](macros.parsh) | Quote templates, `&optional`/`&rest` macro parameters, macro expansion, unquote splicing, and caller scope |

The examples use standard Linux coreutils commands. `files.parsh` creates a
temporary directory with `mktemp -d`, writes its example files there, and removes
the directory on successful completion.
