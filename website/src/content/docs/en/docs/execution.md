---
title: "Execution results & tasks"
description: "Callable commands, complete results, the $? variable, exceptions, and native background tasks."
---

## Callable commands

Functions, ordinary builtins, and external commands share a call interface. `command`
creates an explicit external reference, even if a function has the same name:

```parsh
(let print (command printf))
($print "<%s>\n" "a b")
(apply $print "<%s>\n" [first second])
(dump (apply + [#1 #2]))
```

`apply` accepts a function, command reference, or text name. The text may be a
quoted string, so namespace names can be selected dynamically:

```parsh
(dump (apply "str/join" [[a b] ","]))
(dump (apply "map/get" [{name parsh} name]))
```

`function` obtains only user functions and ordinary builtins. `command` stores a
program name without searching PATH or starting it; lookup happens when called.
It always bypasses language definitions. `command?` identifies command references;
`callable?` identifies function and command reference values, not text names.
Text names follow ordinary lookup and can also select struct constructors,
accessors, and predicates. Macros and special forms such as `if` cannot be passed
as ordinary callees.
Arguments evaluate once from left to right, and values passed by `apply` are never
reparsed as source.

Ordinary commands stream stdout/stderr and return `CallResult`; both stream fields
are `#nil` by default. `(let result (pwd))` prints the directory and binds the
result structure. Value position never implies capture. External commands,
pipelines, `type`, `cd`, and `export` share this result type. Arithmetic and data
builtins retain their types; user functions return their body value, including a
command's `CallResult` when it is the final expression.

Use `(<-out pwd)` or `(<-err callee args...)` for text. These
capture only the selected stream, decode UTF-8 strictly, and remove leading and
trailing Unicode whitespace by default. The other stream keeps its current
destination. Empty or whitespace-only output returns empty text; an uncaptured
or explicitly redirected stream returns nil. Command failures throw with the
captured result attached.

The three capture functions, `<-out`, `<-err`, and `<-result`, accept an optional
**leading options map**, before the callee. These are ordinary builtins supporting
`function` and `apply`.

| Option | Default | Behavior |
| --- | --- | --- |
| `trim` | `#true` | Remove surrounding Unicode whitespace from captured text; `#false` preserves it |
| `bytes` | `#false` | Return raw bytes; `#true` skips UTF-8 decoding and ignores trim |

Option values must be booleans; unknown keys are errors. The options affect both
stream fields in `<-result`, including its nonzero-exit results. A map
passed after the callee remains an ordinary argument.

```parsh
(dump (<-out printf " hello\n"))
(dump (<-out {trim #false} printf " hello\n"))
(dump (bytes/hex-encode (<-out {bytes #true} printf " hello\n")))
(dump (apply <-out [{trim #false} printf " hello\n"]))
```

Results are `"hello"`, `" hello\n"`, `"2068656c6c6f0a"`, and `" hello\n"`.
`{bytes #true trim #true}` still returns every original byte.

| Call | Arguments and result |
| --- | --- |
| `(command name)` | One nonempty text program name; returns `<command name>` |
| `(apply callee fixed... args)` | Expands only the final list into call arguments |
| `(<-out [options] callee args...)` | Captured stdout; trimmed text by default, or raw bytes |
| `(<-err [options] callee args...)` | Captured stderr; trimmed text by default, or raw bytes |
| `(<-result [options] callee args...)` | Complete synchronous `CallResult`; captured fields use the same options |
| `(spawn callee args...)` | Flat arguments; returns a native `Job` |

Lists passed to capture helpers and `spawn` remain one language argument; external
command calls then apply the usual one-level list expansion to argv. The callee
is a function value, command reference, or text name, rather than an unevaluated
command expression. Wrap multiple expressions or a pipeline in `(fn () ...)`.
Arguments evaluate once from left to right before capture starts.

## Complete results

`<-result` takes an optional leading options map, the callee, and its
arguments. It returns a `CallResult`, capturing stdout and stderr independently:

```parsh
(let result (<-result printf "hello\n"))
(dump (list (stdout-text $result) (exit-code $result) (signal $result)))
(let sum (<-result + #2 #3))
(dump (. $sum value))
```

| Field | Meaning |
| --- | --- |
| `value` | Language return value; nil for an external command |
| `termination` | `(Exited code)`, `(Signaled signal core_dumped)`, or nil |
| `stdout`, `stderr` | Trimmed text by default; raw bytes with `{bytes #true}`; nil when redirected or not captured |
| `error` | `Error` summary with a nil `result` field, or nil |
| `stages` | `StageResult` records containing argv, termination, and error for pipeline stages |

Runtime records provide ordinary field accessors and predicates, such as
`CallResult.stdout`, `CallResult?`, and `Exited.code`. Their constructors and names
are reserved; inspect records returned by the runtime rather than constructing them.

The generic `.` builtin reads the same fields: `(. $result stdout)` is equivalent
to `(CallResult.stdout $result)` and preserves its text, bytes, or nil type. Multiple fields
chain from left to right, so `(. $result termination code)` reads a nested exit code.
It also accepts dynamic text field names, such as `(. $result $field)`. Field reads
do not replace `$?`.

| Record | Fields |
| --- | --- |
| `CallResult` | `value`, `termination`, `stdout`, `stderr`, `error`, `stages` |
| `Exited` | `code` |
| `Signaled` | `signal`, `core_dumped` |
| `StageResult` | `argv` (text list), `termination`, `error` |
| `Error` | `kind`, `message`, `code`, `offset` (byte offset), `payload`, `result` |
| `Job` | `session`, `id` |
| `JobStatus` | `state`, `result` |

`HttpResponse` is another runtime-owned record; see [HTTP](/parsh/en/docs/stdlib/#http).

`bytes?` identifies raw captured bytes, `typeof` reports `bytes`, and
`eq` compares their contents. Bytes display as `<bytes N>` and cannot implicitly
become text, external arguments, or environment values.

`stdout-text` and `stderr-text` return stored text unchanged or strictly decode
stored bytes as UTF-8, without additional trimming. They return nil for an
uncaptured or redirected stream, and empty text for an empty capture. Invalid
UTF-8 raises an error carrying the original raw result. Capture decoding failures
also publish their raw bytes in `$?` and attach them to the error. A command or
transport failure retains its original kind if partial output cannot be decoded.
`exit-code` returns nil for signal termination or absent termination information;
`signal` returns nil for normal exit or absent termination information. A program exiting with code 143 differs from one killed by SIGTERM. Core-dump information is retained
in `Signaled.core_dumped` on native Linux.

Nonzero exits and terminating signals are returned as data by `<-result`.
Startup, argument, and language errors still throw. A failing command inside a
function stops that body; capture does not run the remaining expressions. Use
`check` to throw on a failed result; successful checking returns the same structure:

```parsh
(try
  (check (<-result cat missing.txt))
  (catch err (echo "read failed: $(Error.message $err)")))
```

Explicit file redirection takes precedence over capture. Captured streams share an
8 MiB limit; overflow stops execution and retains partial output in the error's
result. Uncaptured foreground commands keep streaming. Pipelines still accept only
external stages; wrap a pipeline in a function to capture or spawn it:

```parsh
(dump (<-result (fn () (| (printf "b\na\n") (sort)))))
```

Select bytes explicitly to capture binary output without decoding or trimming:

```parsh
(let binary (<-result {bytes #true} printf "\\377\\000\\101"))
(dump (bytes/hex-encode (. $binary stdout)))
```

The result is `"ff0041"`. Use the [bytes functions](/parsh/en/docs/stdlib/#bytes)
for slicing, concatenation, and explicit text conversion.

## The previous result: `$?`

`$?` is a read-only, session-local `CallResult`, initially nil. `(get ?)` reads the
same value. It is a complete structure, so obtain the numeric exit code explicitly:

```parsh
(let here (<-out pwd))
(let saved $?)
(dump (list $here (exit-code $saved) (signal $saved)))
(echo "exit=$(exit-code $?)")
```

External commands, pipelines, user functions, and command builtins such as `cd`,
`export`, and `type` publish results. Failures publish before throwing. Pure data
builtins, field access, variable reads, and task-status queries do not replace the
result. Binding, control-flow, and `apply` wrappers preserve their target's result.
Capture helpers, `wait`, and `check` publish the result they handle.

Normal calls leave both stream fields nil. Explicit capture retains the selected text or bytes;
terminal output is never silently buffered. Save `$?` in a binding to keep a snapshot
before another command. Neither assignment, export, nor parameters can use `?`.
Quoted interpolation recognizes `$?` but does not implicitly stringify its structure;
use a field expression. Single quotes and `\$` preserve literal dollars.

## Exceptions and cleanup

```parsh
(try
  (throw {reason expected})
  (catch err (echo "caught $(Error.kind $err)"))
  (finally (echo cleanup-finished)))
```

`Error` exposes `kind`, `message`, `code`, `offset`, `payload`, and `result`. Throwing
an existing Error rethrows it; other values become the payload of a user error.
`try` requires a body and at least one of `catch` or `finally`. Each clause occurs
at most once, catch precedes finally, and clauses have separate lexical scopes.
The entire clause structure is validated before the body runs.

Catch handles runtime errors, not return, break, continue, exit, or cancellation.
Finally runs on normal completion, exceptions, and language jumps. Its ordinary
value is ignored; a new exception overrides the pending outcome, except cancellation.
Return, break, and continue are forbidden in the finally block's current function;
helper functions may return normally. Forced cancellation can interrupt cleanup.

See [errors and exit codes](/parsh/en/docs/diagnostics/) for error kinds and the
difference between script reading and interactive batch validation.

## Native background tasks

```parsh
(let job (spawn printf "hello %s\n" world))
(dump (job-status $job))
(let completed (wait $job))
(dump (stdout-text $completed))
(dump (check $completed))
```

`spawn` accepts external commands, user functions, and ordinary builtins with flat
arguments. It returns a session-owned `Job`, not a result that later mutates.
`JobStatus` snapshots have `state` (`running` or `completed`) and `result` (nil until
complete). `wait` returns the complete result, including asynchronous language errors;
repeat waits retain the same result and returned closure identities. `cancel` requests
cancellation and returns nil. Jobs belong to their original session.

Each task owns raw bytes for stdout/stderr capture and defaults stdin to EOF. Explicit redirection
wins. Functions run in isolated interpreter threads: lexical scopes, recursive
closures, cwd, definitions, and environment are copied; task changes do not alter
parent state. Task results can contain closures. An external startup failure throws
from spawn; a later function error is stored in its result. Exit inside a task ends
only that task.

Spawn publishes a launch result with the Job in `value` and nil termination. Background
completion changes only task state; parent `$?` changes when it executes `wait`.
Tasks have their own `$?`. Session exit or reset cancels and reaps owned tasks and
subprocesses. Cancellation is cooperative for interpreter threads, with a 200 ms
cleanup window before forced evaluation cancellation and subprocess termination;
blocking native I/O can delay thread cleanup. Threads are never forcibly killed.

Background execution requires native parsh. The browser supports synchronous calls,
results, bytes, `$?`, and exceptions, and reports background interfaces as unsupported.
There is no literal `&`, job suspension, `fg/bg`, or detached persistent task support.

| Call | Result |
| --- | --- |
| `(wait job)` | Final `CallResult`; waits until completion and can be repeated |
| `(job-status job)` | `JobStatus` snapshot without changing `$?` |
| `(cancel job)` | Request cancellation; returns nil without changing `$?` |

Explicit captures can nest up to 32 levels; evaluation has a separate 128-level
limit. Runnable examples are listed in the [examples guide](/parsh/en/docs/examples/).
