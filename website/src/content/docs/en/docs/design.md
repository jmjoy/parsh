---
title: "Design principles"
description: "Why parsh trades shell conventions for explicit values and structure."
---

parsh does not have a formal manifesto. These are the principles the
implementation keeps returning to, stated as plainly as possible:

> Replace the string conventions of a shell with expressions, values, and
> explicit boundaries, and accept a little verbosity in exchange for
> predictability.

## One language, two entry points

The prompt is an expression with the opening parenthesis implied. `$ ()` waits
for `echo hi`, which evaluates `(echo hi)`. Interactive input and scripts share
the same reader, evaluator, and diagnostics, so there is no separate
command-line grammar to learn or to keep in sync.

Comments follow the same rule: `(; ...)` is a form that nests and obeys bracket
balancing, instead of a line comment that ends at a newline. See
[syntax and values](/parsh/en/docs/syntax/) for the details.

## Text first, types explicit

Bare words are text. `123` is the text `123`, and `true`, `false`, and `nil`
have no literal meaning in value positions. Numbers appear when arithmetic or a
numeric comparison parses numeric text, and booleans come from comparisons,
`not`, or a command status:

```parsh
(eq 001 1)
(= 001 1)
(let mode fast)
(if (eq $mode fast) (echo fast))
```

A condition must produce a boolean or a command status. Text is never
implicitly truthy.

## No implicit conversions

Text stays one argument. Lists expand one level into external command
arguments. Globbing is a call, not a syntax rule:

```parsh
(let files (list "a file.rs" "b.rs"))
(printf "<%s>\n" $files)
(for file (glob "*.rs") (echo $file))
```

The point is that what a script says is what it runs: no word splitting, no
hidden pathname expansion, and no guessing what spaces mean at the call site.

## Commands are expressions, programs are opaque

An expression can call a built-in, a macro, a function, a struct constructor, or
a program. External programs start directly: the expression is not handed to
another shell to parse again, so there is no second round of quoting and no
injection layer.

Command results are values. A command used as a value captures stdout as UTF-8
text with trailing newlines removed, binary output goes through redirection or
pipelines, and the exit status is a `status` value. A nonzero status stops the
current batch, but conditions may inspect it: failures are data, not exceptions.
See [commands and composition](/parsh/en/docs/commands/).

## Value semantics and lexical scope

Values copy when read, assigned, or passed, including nested lists and maps.
Collection updates return new values instead of mutating old ones:

```parsh
(let config {mode fast})
(set config (mset $config mode slow))
```

Functions capture their defining scope, and loops create a fresh scope per
iteration. This removes a class of questions about where a value was changed,
and it lets structs and macros compose without aliasing surprises.

## Structure before convention

Lists, maps, structs, pipelines, and redirections are all expressions. `type`
reports how a name resolves without running it, and completion covers commands,
paths, variables, struct members, and macros:

```parsh
(defstruct Point x y)
(type Point.x)
(| (find . -name "*.rs") (sort))
```

The goal is for people and tools to read structure rather than guess at
strings. The same property makes generated scripts easier to review: one
grammar and no quoting layers, so the script reads the way it runs. See
[structs and macros](/parsh/en/docs/structs-and-macros/).

## Bounded and documented

The language says what it does not do yet. Pipeline stages accept external
commands only; macros have fixed arity and no hygiene; nested quasiquotes are
copied literally; background jobs and library imports are future work. Exit
codes and diagnostic positions are stable enough to depend on: language errors
use `2`, a missing program `127`, permission denied `126`. See
[errors and exit codes](/parsh/en/docs/diagnostics/).

Limits are documented rather than discovered.

## What it rejects

- Structure carried by spaces and quotes.
- Text treated as truth.
- Implicit word splitting and pathname expansion.
- Re-parsing expressions inside another shell.
- Undocumented behavior presented as compatibility.

## The costs

These choices are not free:

- One-off commands are wordier: `(if (eq $x y) ...)` instead of `[ "$x" = y ]`.
- There are no numeric literals; `(echo 1)` prints text, and numbers appear in
  arithmetic contexts.
- Copy semantics make large collections expensive to pass around.
- POSIX habits do not transfer: `for f in *.rs` becomes `(for f (glob "*.rs"))`.

The trade is deliberate: a little more typing for a script whose behavior can
be read off the page.
