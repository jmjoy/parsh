---
title: "Variables & scope"
description: "Bindings, environment values, lists, and explicit expansion."
---

In value positions, bare words are text: `123`, `true`, `false`, and `nil` have no
special literal meaning. Double-quoted strings interpolate `$name` and `$(...)`,
and support `\"`, `\\`, `\n`, `\r`, `\t`, and `\$`. Single-quoted strings preserve
their content without interpolation or escapes. `${name}` remains literal text.
Triple quotes preserve newlines and indentation: `"""..."""` supports the same
interpolation and escapes, while `'''...'''` preserves all content literally.
Outside strings, variable references use `$name`. Binding
and assignment targets use bare names, including function parameters, loop
bindings, and environment settings. Names start with a letter or `_`, followed
by letters, digits, or `_`. Binding targets must be bare names, not quoted strings
or expressions.

```parsh
(let name "a friend")
(echo hello $name)
(set name world)
(echo "hello, $name")
(export EDITOR vim)
```

`let` binds in the current lexical scope. `set` updates the nearest existing
binding, or an existing environment entry, and errors if the name is undefined.
Lookups fall back to the inherited environment. `export` sets the environment for
subsequent children and updates an existing local binding of the same name.
Environment and working directory changes belong to this parsh session.

`get` takes exactly one text variable name, which can itself be an expression,
and returns the original value using the same lookup rules as `$name`:

```parsh
(get name)
(get (str na me))
(echo "hello, $(get name)_friend")
```

Names must follow the variable naming rules above. Undefined names are errors.
`get` preserves lists, numbers, booleans, and nil; conversion to text happens
only when used inside a string. `get` is a reserved built-in command name.
For expression interpolation and literal dollars, see [strings](/parsh/en/docs/syntax/#strings-and-interpolation).

```parsh
(let files (list "a file.rs" other.rs))
(printf "<%s>\n" $files)
(for file (glob *.rs)
  (echo $file))
```

Lists expand by one level into external command arguments. Text stays a single
argument, including spaces and embedded newlines. `list` constructs lists;
`str` concatenates scalar values, converting numbers with the same format used
for command arguments and environment values. `glob` is explicit, returns sorted paths, and
returns an empty list when nothing matches. Ordinary words, strings, and variable
values never undergo implicit globbing, word splitting, or tilde expansion.

`cd` accepts a path, or defaults to `$HOME`; it updates `$PWD` and `$OLDPWD`.
Relative commands, glob patterns, and redirections use the current directory.
