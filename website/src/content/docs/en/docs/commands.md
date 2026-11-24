---
title: "Commands & composition"
description: "Native commands, pipelines, output capture, and file redirection."
---

:::note[Native Linux]
This page covers command composition in parsh. The browser playground supports a coreutils subset, temporary files, and buffered pipelines; see its [capabilities](/parsh/en/docs/playground/). Other Linux programs require native parsh.
:::

Command lookup checks built-ins, macros, struct constructors and members, named
functions, then PATH. External programs
are started directly, without passing the expression through another shell.
The command name may also come from a text variable, as in `($EDITOR file.txt)`;
its value is one program name, without implicit splitting into flags.

Command heads accept text, function values, and explicit `command` references. Numeric, boolean, and nil heads are not callable, including values read from variables. Use `str` to explicitly construct a text command name. `int`, `float`, and `bool` are reserved builtins.

Use `type` to inspect command lookup without executing the command:

```parsh
(type type cd echo)
(defn greet (name) (echo hello $name))
(type greet)
(type -a echo)
(let kind (<-out type -t greet))
(dump (if (type -t optional-tool) (optional-tool)))
```

`type` accepts multiple names, including list arguments, and supports these Bash-style options:

- `-a`: show every match, including all executable paths in PATH order.
- `-t`: print `builtin`, `macro`, `struct`, `function`, or `file` for each match.
- `-p`: print the external command path, or nothing for a built-in, macro, struct, or function.
- `-P`: search PATH even when a built-in or function has the same name.
- `-f`: skip function, macro, and struct lookup.

Options can be combined; `--` ends option parsing. No arguments succeeds silently.
An unknown name gives status `1`; an invalid option gives status `2`.
Output supports capture and redirection. In the browser playground, external
commands are reported by the host's available command names; `-p` and `-P`
return those names.

```parsh
(echo (<-out pwd))
(| (find . -name "*.rs") (sort))
(out-> files.txt (| (find . -name "*.rs") (sort)))
(out->> log.txt (echo another entry))
(in<- files.txt (sort))
(err-> errors.txt
  (if (cat missing.txt) (echo found) (echo "read failure handled")))
```

The operators are:

- `|`: connect external commands in a pipeline.
- `out->`: create or truncate a file and redirect stdout.
- `out->>`: create or append a file and redirect stdout.
- `in<-`: read stdin from a file.
- `err->`: create or truncate a file and redirect stderr.
- `err->>`: create or append a file and redirect stderr.

Redirection always takes the path first and an expression second. Wrappers can
nest and can enclose an entire pipeline. Pipeline stages must call external
commands. Their heads can be text names, text variables, or `command` references;
user functions, macros, struct members, and builtins cannot run as stages.
Use an explicit reference to bypass a same-named language command:

```parsh
(let sorter (command sort))
(| (printf "b\na\n") ($sorter))
```

For example, write stdout to a fresh log and append stderr to another:

```parsh
(out-> output.log
  (err->> errors.log
    (make)))
```

Ordinary commands write to their current destinations and return `CallResult` in
all positions, including assignments, arguments, and interpolation. Their stdout
and stderr fields are `#nil` by default. Use `<-out` or `<-err`
for one stream, or `<-result` for both streams and a complete result.
Captured text is trimmed by default; a leading `{trim #false}` keeps whitespace,
and `{bytes #true}` returns original bytes without trimming.
Explicit file redirection takes precedence over capture; pipelines and
files preserve binary bytes.

Use `<-|out`, `<-|err`, or `<-|result` to capture a single pipeline without an
outer zero-argument function:

```parsh
(let count (int (<-|out
  (find . -name "*.rs")
  (wc -l))))
```

They are equivalent to wrapping `(| ...)` in `(fn () ...)` and passing it to
`<-out`, `<-err`, or `<-result`.

For complete results, raw-byte capture, `$?`, exceptions, and background execution, see [execution results and tasks](/parsh/en/docs/execution/).
