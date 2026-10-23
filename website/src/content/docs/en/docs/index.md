---
title: "Meet parsh"
description: "A Linux shell built around S-expressions."
---

**parsh** stands for **parenthesis shell**. It is a Linux shell whose command line and scripts use the same small Lisp dialect.

The prompt is `$ ()`. Type `str hello` and press Enter to evaluate `(str hello)`. Enter continues unfinished input on a new line; Ctrl+J always inserts a newline.

```parsh
(let name world)
(defn greet (who)
  (str "hello, " $who))
(greet $name)
```

The interactive result is `"hello, world"`. Scripts use exactly the same syntax; on Linux, expressions can also launch real programs, build pipelines, and redirect files.

## Pick a starting point

- [Get started on Linux](/parsh/en/docs/getting-started/) to use parsh with your own commands.
- [Try the language](/parsh/en/playground/) without installing anything.
- [Learn the syntax](/parsh/en/docs/syntax/) and build up to functions and command composition.

## Current scope

parsh is an early implementation, version 0.1.0. Native parsh supports Linux foreground jobs, including interactive programs and pipelines. Interactive Tab completion covers commands, paths, and variables. Background jobs, macros, and persistent configuration are future work. The browser playground offers the language core, 29 coreutils commands, temporary files, and command composition.

Licensed under the [Mulan Permissive Software License, Version 2](https://github.com/jmjoy/parsh/blob/master/LICENSE).
