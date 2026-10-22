---
title: "Commands & composition"
description: "Native commands, pipelines, output capture, and file redirection."
---

:::note[Native Linux]
This page covers command composition in parsh. The browser playground supports a coreutils subset, temporary files, and buffered pipelines; see its [capabilities](/parsh/en/docs/playground/). Other Linux programs require native parsh.
:::

Command lookup checks built-ins, named functions, then PATH. External programs
are started directly, without passing the expression through another shell.
The command name may also come from a text variable, as in `($EDITOR file.txt)`;
its value is one program name, without implicit splitting into flags.

```parsh
(echo (pwd))
(| (find . -name "*.rs") (sort))
(|> files.txt (| (find . -name "*.rs") (sort)))
(|>> log.txt (echo another entry))
(<| input.txt (sort))
(|2> errors.txt (grep needle missing.txt))
```

The operators are:

- `|`: connect external commands in a pipeline.
- `|>`, `|1>`: create or truncate a file and redirect stdout.
- `|>>`, `|1>>`: create or append a file and redirect stdout.
- `<|`: read stdin from a file.
- `|2>`: create or truncate a file and redirect stderr.
- `|2>>`: create or append a file and redirect stderr.

Redirection always takes the path first and an expression second. Wrappers can
nest and can enclose an entire pipeline. Pipeline stages in v1 must be external
command calls; functions and state-changing built-ins run outside pipelines.

For example, write stdout to a fresh log and append stderr to another:

```parsh
(|1> output.log
  (|2>> errors.log
    (make)))
```

Top-level commands stream output immediately. When an external command or
pipeline is used as a value, stdout is captured as UTF-8, trailing LF characters
are removed, and the result stays one argument. Stderr keeps its existing
destination. Use a pipeline or file redirection for binary output. Explicit
redirection takes precedence over capturing that expression's output.
