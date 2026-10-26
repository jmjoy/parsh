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
(apply + [#1 #2])
```

`apply` accepts a function, command reference, or text name. `function` continues to
obtain only user functions and ordinary builtins. `command?` and `callable?` inspect
references. Names follow ordinary lookup; macros and special forms such as `if`
cannot be passed as ordinary callees. Arguments evaluate once from left to right,
and values passed by `apply` are never reparsed as source.

Ordinary calls keep their existing behavior: `(let here (pwd))` obtains stdout as
UTF-8 text with trailing LF characters removed, while arithmetic and typed functions
return their language values. A function ending with a command can return its captured
output; an explicit typed return retains that value.

## Complete results

`capture-call` takes the callee followed directly by its arguments and returns a
`CallResult`. It captures stdout and stderr independently:

```parsh
(let result (capture-call printf "hello\n"))
(list (stdout-text $result) (exit-code $result) (signal $result))
(let sum (capture-call + #2 #3))
(CallResult.value $sum)
```

| Field | Meaning |
| --- | --- |
| `value` | Language return value; nil for an external command |
| `termination` | `(Exited code)`, `(Signaled signal core_dumped)`, or nil |
| `stdout`, `stderr` | Captured `bytes`, or nil when explicitly redirected or not captured |
| `error` | Error summary, or nil |
| `stages` | `StageResult` records containing argv, termination, and error for pipeline stages |

These are runtime structures with ordinary field accessors and predicates, such as
`CallResult.stdout`, `CallResult?`, and `Exited.code`. Their constructors and names
are reserved. `bytes?` identifies raw captured bytes, `typeof` reports `bytes`, and
`eq` compares their contents. Bytes display as `<bytes N>` and cannot implicitly
become text, external arguments, or environment values.

`stdout-text` and `stderr-text` strictly decode UTF-8 and preserve newlines. Invalid
UTF-8 raises an error carrying the original result. `exit-code` returns nil for
signal termination; `signal` returns nil for normal exit. Thus a program exiting
with code 143 differs from one killed by SIGTERM. Core-dump information is retained
in `Signaled.core_dumped` on native Linux.

Nonzero exits and terminating signals are returned as data by `capture-call`.
Startup, argument, and language errors still throw. A failing command inside a
function stops that body; capture does not run the remaining expressions. Use
`check` to throw on a failed result; successful checking returns the same structure:

```parsh
(try
  (check (capture-call cat missing.txt))
  (catch err (echo "read failed: $(Error.message $err)")))
```

Explicit file redirection takes precedence over capture. Captured streams share an
8 MiB limit; overflow stops execution and retains partial output in the error's
result. Uncaptured foreground commands keep streaming. Pipelines still accept only
external stages; wrap a pipeline in a function to capture or spawn it:

```parsh
(capture-call (fn () (| (printf "b\na\n") (sort))))
```

## The previous result: `$?`

`$?` is a read-only, session-local `CallResult`, initially nil. `(get ?)` reads the
same value. It is a complete structure, so obtain the numeric exit code explicitly:

```parsh
(let here (pwd))
(let saved $?)
(list $here (exit-code $saved) (signal $saved))
(echo "exit=$(exit-code $?)")
```

External commands, pipelines, user functions, and command builtins such as `cd`,
`export`, and `type` publish results. Failures publish before throwing. Pure data
builtins, field access, variable reads, and task-status queries do not replace the
result. Binding, control-flow, and `apply` wrappers preserve their target's result.
`capture-call`, `wait`, and `check` publish the result they handle.

A normal call records only output already captured by its value context; it does
not silently buffer terminal output. Save `$?` in a binding to keep a snapshot
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

## Native background tasks

```parsh
(let job (spawn printf "hello %s\n" world))
(job-status $job)
(let result (wait $job))
(stdout-text $result)
(check $result)
```

`spawn` accepts external commands, user functions, and ordinary builtins with flat
arguments. It returns a session-owned `Job`, not a result that later mutates.
`JobStatus` snapshots have `state` (`running` or `completed`) and `result` (nil until
complete). `wait` returns the complete result, including asynchronous language errors;
repeat waits retain the same result and returned closure identities. `cancel` requests
cancellation and returns nil. Jobs belong to their original session.

Each task owns stdout/stderr capture and defaults stdin to EOF. Explicit redirection
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

Explicit captures can nest up to 32 levels; ordinary evaluation retains its separate depth limit.
