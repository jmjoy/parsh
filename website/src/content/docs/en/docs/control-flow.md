---
title: "Functions & control flow"
description: "Lexical functions, branching, sequencing, and iteration."
---

```parsh
(defn greet (name)
  (echo hello $name))

(if (test -d src)
    (greet developer)
    (echo missing))

(if (eq $EDITOR vim)
    (echo using vim))

(if (not (test -f config.txt))
    (echo configuration missing))

(do
  (let message "multiple expressions")
  (echo $message))
```

`if` evaluates only the selected branch. A condition must produce a boolean
(from `eq`, a numeric comparison, or `not`) or a command status; zero status is true. Text and
numbers are not implicitly booleans. `do` runs expressions in sequence in the current scope.
`for` evaluates a list and creates a fresh lexical binding for each iteration.

Function names and parameter declarations use bare names. Read parameter and
local variable values with `$name`. Functions capture their defining lexical
scope and return the last expression's value. Argument expressions are evaluated
before parameters are bound. Functions can return numbers, lists, and booleans as well as
command output.
When called as a value, stdout is captured throughout the function body: if the
last expression produces a command status, captured stdout is returned as text;
otherwise the last typed value is returned. Earlier captured output is not
printed in that case.

## See the return value

```parsh
(defn greet (who)
  (str "hello, " $who))
(greet parsh)
(if (eq a a) (list yes) (list no))
(for item (list first last) (str $item))
```

An interactive session displays `"hello, parsh"`, `("yes")`, and `"last"`. A loop returns the last body's result, rather than collecting every iteration. These examples work in the browser playground.
