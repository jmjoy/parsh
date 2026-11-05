# parsh examples

English | [简体中文](README.zh-CN.md)

Run these scripts from the repository root with native parsh. Each script also includes its run command:

```console
cargo run -- examples/hello.parsh world
cargo run -- examples/functions.parsh
cargo run -- examples/maps.parsh
cargo run -- examples/execution.parsh
cargo run -- examples/tasks.parsh
```

Once `parsh` is on PATH, run a script directly, for example `./examples/while.parsh`.
Scripts do not automatically print expression return values; these examples use
`echo`, `printf`, or `dump`. `dump` also displays lists, maps, structs, functions, and bytes.

| Script | Demonstrates | Environment |
| --- | --- | --- |
| [hello.parsh](hello.parsh) | Script arguments, named functions, and command conditions | Native only |
| [variables.parsh](variables.parsh) | Bindings, interpolation, raw strings, lists, get, and export | Native and browser |
| [literals.parsh](literals.parsh) | Typed # literals and explicit int/float/bool conversion | Native and browser |
| [arithmetic.parsh](arithmetic.parsh) | Arithmetic, numeric comparisons, and text equality | Native and browser |
| [functions.parsh](functions.parsh) | Recursion, lexical scope, closures, optional/rest parameters, return, and apply | Native and browser |
| [for.parsh](for.parsh) | List iteration, fresh scopes, continue, break, and nested loops | Native and browser |
| [while.parsh](while.parsh) | Conditional iteration, outer bindings, and loop return values | Native and browser |
| [structs.parsh](structs.parsh) | Constructors, accessors, predicates, nesting, and value equality | Native and browser |
| [macros.parsh](macros.parsh) | Quoted forms, quasiquote, splicing, optional/rest parameters, and caller scope | Native and browser |
| [stdlib.parsh](stdlib.parsh) | Immutable lists, Unicode text, higher-order transforms, and numeric helpers | Native and browser |
| [maps.parsh](maps.parsh) | Immutable updates, eager defaults, ordered keys, merge, and entries | Native and browser |
| [json.parsh](json.parsh) | Struct encoding, typed decoding, byte input, and JSON errors | Native and browser |
| [bytes.parsh](bytes.parsh) | Binary values, UTF-8, slicing, concatenation, Base64, and hex | Native and browser |
| [pipelines.parsh](pipelines.parsh) | Pipelines, captured text, and numeric conversion at command boundaries | Native and browser |
| [files.parsh](files.parsh) | Temporary directories, glob, redirection, and finally cleanup | Native only |
| [execution.parsh](execution.parsh) | Command references, complete results, $?, raw capture, check, and exceptions | Native and browser |
| [tasks.parsh](tasks.parsh) | Background execution, isolated state, wait, job-status, and cancellation | Native only |
| [http.parsh](http.parsh) | Native HTTP responses, headers, byte bodies, and JSON decoding | Native only |

The browser column means you can paste the script into the [playground](https://jmjoy.github.io/parsh/en/playground/)
and press Enter. Use a fresh session for each script to avoid duplicate `let` bindings.
The browser has no script-file entry point, `$args` is empty, and pipelines buffer one stage at a time.
`hello.parsh` uses script arguments and repository paths; `files.parsh` uses `mktemp`,
which is absent from the browser. Background tasks and HTTP requests require native parsh.

External commands come from standard Linux coreutils. `files.parsh` writes in a temporary
directory and uses `finally` to restore the working directory and remove its files,
including after a runtime error in its body. `tasks.parsh` starts and cancels a `sleep 30`;
you do not need to wait the full 30 seconds.

`http.parsh` requires an explicit HTTP/HTTPS URL. Without arguments it only prints usage
and makes no request:

```console
cargo run -- examples/http.parsh http://127.0.0.1:8080/data
```

Replace the example URL with a reachable service. The script prints response status and
headers, then decodes JSON or UTF-8 text according to `Content-Type`. HTTP 4xx/5xx
statuses remain response data.

See the [examples guide](https://jmjoy.github.io/parsh/en/docs/examples/) for the related language chapters.
