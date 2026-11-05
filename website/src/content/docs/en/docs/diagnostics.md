---
title: "Errors & exit codes"
description: "Source positions, error kinds, batch failures, and native exit statuses."
---

Diagnostics include the source, line, and column:

```text
interactive:1:6: undefined variable $missing
```

## Command failures and exit codes

An ordinary nonzero command status stops the current script or submitted batch.
Conditions may inspect nonzero statuses, but missing programs and other startup
errors still throw. A native pipeline reports its rightmost nonzero stage status,
even if the last stage succeeds, including upstream SIGPIPE. Every stage is reaped.

| Status | Meaning |
| --- | --- |
| `0` | Successful execution |
| Program's exit code | Ordinary command failure; also propagated from a failed pipeline stage |
| `2` | Reader, language, or CLI error |
| `126` | Command cannot be executed because permission is denied |
| `127` | Command not found |
| `128 + signal` | Native signal termination's compatibility status |

External programs can also choose these exit codes. Complete results distinguish
normal exits from signals: use `exit-code` and `signal` on `$?` or a result returned
by `capture-call` or `wait`. `$?` is a `CallResult`, not a numeric status.

`exit` accepts a code from 0 to 255, defaulting to 0. It exits the session from
nested expressions or functions after unwinding `finally` clauses; `catch` does
not intercept it. Inside a background task, it exits only that task.

## Reader errors and batches

An unexpected top-level closing delimiter (`)`, `]`, or `}`) is consumed and
diagnosed, then reading continues. A mismatched delimiter inside an expression is
an error; unfinished source reports its unclosed opening delimiter.

File, stdin, and `-c` scripts read and execute incrementally, so earlier expressions
can run before a later syntax error is found. Reader errors set status 2 unless a
runtime failure or explicit exit supersedes them. After a runtime failure, the
reader can still diagnose later source, but no later expression executes.

The native editor and browser playground validate the entire submitted batch
before executing any expression. A syntax error preserves the draft; fix or clear
it before submitting new input. A runtime error stops the current batch while
retaining earlier effects; a fresh input can still run. Reset after a browser
session exits. Commands absent from the browser produce a capability diagnostic.

## Catching errors

`try/catch/finally` handles runtime errors. `capture-call` and `wait` return
inspectable results for command failure; `check` throws if the result failed.
The caught `Error` exposes `kind`, `message`, `code`, `offset`, `payload`, and `result`:

| Kind | Typical cause |
| --- | --- |
| `runtime` | Undefined variable, invalid operand, or strict UTF-8 result decoding |
| `command-failed` | Nonzero normal command exit |
| `signaled` | Native subprocess terminated by a signal |
| `spawn-failed` | An external program could not be started |
| `output-limit` | Captured output exceeded 8 MiB |
| `json`, `bytes`, `http` | Failure in the corresponding data or request operation |
| `unsupported` | A capability is unavailable in this runtime |
| `user` | A non-Error value was passed to `throw` |
| `cancelled` | A native task was cancelled |

Reader errors cannot be caught by `try`. See [execution results and tasks](/parsh/en/docs/execution/)
for recovery, typed payloads, cleanup, and termination information.

## Limits

The reader permits up to 256 nested levels. Evaluation is limited to 128 levels;
macro expansion and explicit capture each have separate 32-level limits.
Captured stdout and stderr share an 8 MiB budget. Browser-specific output,
filesystem, and execution-time limits are listed under [playground capabilities](/parsh/en/docs/playground/).
