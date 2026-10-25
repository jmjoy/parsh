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
- [Define structs and macros](/parsh/en/docs/structs-and-macros/) to extend the language itself.

## AI-friendly

Shell scripts often push their logic into `awk`, `sed`, or an inline interpreter, and each layer brings its own quoting rules. parsh keeps it all in the script: arithmetic, lists and maps, functions, and loops behave as in a general-purpose language, while [external commands](/parsh/en/docs/commands/), pipelines, and redirections share the same syntax. Values are never split or globbed implicitly, so a script means exactly what it says.

```parsh
(let count (+ (| (find . -name "*.rs") (wc -l)) 0))
(echo "rust files: $count; doubled: $(* $count 2)")
```

One grammar and one set of rules to generate against and to check: that is what makes parsh scripts a good fit for AI-assisted work. Library imports are planned, so scripts will also reuse packaged code the way Python modules do.

## Current scope

parsh is an early implementation, version 0.1.0. Native parsh supports Linux foreground jobs, including interactive programs and pipelines. Interactive Tab completion covers commands, paths, and variables. Background jobs, library imports, and persistent configuration are future work. The browser playground offers the language core, 29 coreutils commands, temporary files, and command composition.

Licensed under the [Mulan Permissive Software License, Version 2](https://github.com/jmjoy/parsh/blob/master/LICENSE).
