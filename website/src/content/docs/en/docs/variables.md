---
title: "Variables & scope"
description: "Bindings, environment values, lists, maps, and explicit expansion."
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
`get` preserves lists, maps, numbers, booleans, and nil; conversion to text is attempted
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

## Maps

`map` accepts alternating text keys and values. `(map)` creates an empty map.
Keys may be bare words, strings, text variables, or expressions returning text;
other key types are errors. Values retain their types and may contain lists or maps.
Keys and values evaluate once from left to right. An odd argument count is rejected
before evaluating any arguments; a duplicate key is rejected at its second occurrence,
before evaluating that key's value.

```parsh
(let config (map name parsh version (+ 1) nested (map enabled (eq a a))))
(mset config name new-name)
(mget config name)
(mget config missing fallback)
(mkeys config)
(mdel config version)
```

Every `m` command takes a bare variable name as its first argument: use `config`,
without `$` or quotes. It accesses the nearest lexical binding. An undefined name
or a binding containing a different type is an error.

| Command | Result |
| --- | --- |
| `(mget config key)` | Original value; a missing key is an error |
| `(mget config key default)` | Evaluate and return `default` only if the key is missing |
| `(mset config key value)` | Insert or replace the value; return nil |
| `(mdel config key)` | Delete the key; return nil; a missing key is an error |
| `(mhas config key)` | Boolean indicating whether the key exists |
| `(mkeys config)` | Text keys in insertion order, as a list |
| `(mvals config)` | Values in the same order, as a list |
| `(mlen config)` | Integer entry count |

An existing key with a nil value is still present, so it does not use the default.
The default does not handle undefined names or non-map bindings.
After validating argument count and the bare name, key and value expressions
evaluate once from left to right, then the command accesses the current binding.
Changes made to the same map by these expressions are preserved.

Updating a key keeps its position. New keys append; deleting preserves the order
of remaining keys, and reinserting appends. Maps display in insertion order as
`(map "name" "parsh")`; `eq` compares contents regardless of that order.

Assignment and function arguments copy maps, including nested values. For example,
after `(let copy $config)`, `(mset config name changed)` does not change `copy`.
Functions can update captured outer bindings, while a parameter with the same name
is a separate binding. `mget` and `mvals` also return independent values.
Maps cannot implicitly become text: `str`, string interpolation, external command
arguments, and environment values reject maps. Use `mkeys` or `mvals` explicitly
when a list is needed for iteration or command arguments.
