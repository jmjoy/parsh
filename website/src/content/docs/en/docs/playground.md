---
title: "The syntax playground"
description: "What the browser session supports and how to use it."
---

[Open the interactive terminal](/parsh/en/playground/) to try the real parsh interpreter compiled from Rust.

## Your first expression

The prompt supplies an opening parenthesis. Type `str "hello, " "parsh")` to evaluate it, or paste complete expressions:

```parsh
(let name world)
(defn greet (who)
  (str "hello, " $who))
(greet $name)
(list a "two words" (eq x x))
```

The results are `"hello, world"` and `("a" "two words" true)`.

## Capabilities

| Feature                                                 | Browser behavior                                             |
| ------------------------------------------------------- | ------------------------------------------------------------ |
| `let`, `set`                                            | Session bindings and lexical scope                           |
| `defn`                                                  | Parameters, closures, and typed return values                |
| `do`, `if`, `for`                                       | Sequences, branches, and loops; loops return the last result |
| `str`, `list`, `eq`, `not`                              | Text, lists, and booleans                                    |
| `export`                                                | An isolated in-memory environment, initially empty           |
| `$args`                                                 | Initially an empty list                                      |
| `exit`, Ctrl+D on empty input                           | End the session; reset to restart                            |
| Files, glob, cd, pipes, redirections, external commands | Require native parsh and produce a diagnostic here           |

There is no `echo` or `printf`. Return text with `str` instead. Nil is not displayed; text displays quotes and escapes. Script mode does not automatically print expression results.

## Sessions and keys

- Enter inserts a newline. A closing outer `)` evaluates the expression.
- Up/Down move between lines or recall history. Ctrl+P/N always browse history. Press `)` at the end of a complete history entry to run it again.
- Unicode editing, multiline input, full-script paste, and batch error recovery use the shared native editor model.
- Clear preserves the session. Reset, refresh, or leaving the page ends the temporary session; browser back-forward caching can temporarily preserve a page.
- Ctrl+C clears idle input. During execution, it immediately stops and resets the entire session.
- An expression running for more than 5 seconds automatically resets the session, clearing variables, functions, environment, and history.

To use commands, files, and pipes, [run parsh on Linux](/parsh/en/docs/getting-started/).
