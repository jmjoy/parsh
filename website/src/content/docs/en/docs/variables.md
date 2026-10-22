---
title: "Variables & scope"
description: "Bindings, environment values, lists, and explicit expansion."
---

In value positions, bare words are text: `123`, `true`, `false`, and `nil` have no
special literal meaning. Double-quoted strings support `\"`, `\\`, `\n`, `\r`, and `\t`.
Strings do not interpolate variables. Variable references use `$name`. Binding
and assignment targets use bare names, including function parameters, loop
bindings, and environment settings. Names start with a letter or `_`, followed
by letters, digits, or `_`. Binding targets must be bare names, not quoted strings
or expressions.

```parsh
(let name "a friend")
(echo hello $name)
(set name world)
(echo (str "hello, " $name))
(export EDITOR vim)
```

`let` binds in the current lexical scope. `set` updates the nearest existing
binding, or an existing environment entry, and errors if the name is undefined.
Lookups fall back to the inherited environment. `export` sets the environment for
subsequent children and updates an existing local binding of the same name.
Environment and working directory changes belong to this parsh session.

```parsh
(let files (list "a file.rs" other.rs))
(printf "<%s>\n" $files)
(for file (glob *.rs)
  (echo $file))
```

Lists expand by one level into external command arguments. Text stays a single
argument, including spaces and embedded newlines. `list` constructs lists;
`str` concatenates scalar values. `glob` is explicit, returns sorted paths, and
returns an empty list when nothing matches. Ordinary words, strings, and variable
values never undergo implicit globbing, word splitting, or tilde expansion.

`cd` accepts a path, or defaults to `$HOME`; it updates `$PWD` and `$OLDPWD`.
Relative commands, glob patterns, and redirections use the current directory.
