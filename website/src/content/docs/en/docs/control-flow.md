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
numbers are not implicitly booleans. Every parenthesized expression creates a fresh
lexical scope, so a `let` in a branch does not leak outside the `if`. `do` creates
a scope and runs its body expressions in sequence; bindings declared directly in
its body stay in that scope. `for` evaluates its list outside the loop, then creates
a fresh lexical scope and loop binding for each iteration.

`while` checks its condition before each iteration. Both loop forms accept one or
more body expressions and create a fresh scope per iteration: `let` stays local,
and `set` can update an outer binding.

`and/or` short-circuit from left to right and return booleans. `(and)` is true;
`(or)` is false. They accept the same conditions as `if`, including command statuses.

```parsh
(let count #0)
(while (< $count #5)
  (set count (+ $count #1))
  (if (= $count #2) (continue))
  (if (= $count #4) (break (list stopped $count)))
  (echo iteration $count))
```

`(continue)` skips the rest of the current iteration. `(break)` ends the nearest
loop; `(break value)` evaluates `value` once and returns it from the loop,
including numbers, lists, maps, booleans, or `nil`. Without an explicit break value,
a loop returns its last complete iteration's result, or `nil` if none completed.
Skipped or interrupted iterations do not replace that result. The example prints
`iteration 1` and `iteration 3`, and returns `("stopped" #4)`.

Loop jumps cannot cross a function boundary. Using one without an active loop
body in the current function is an error. A loop's condition or list expression
runs outside its own body, so jumps there target an existing outer loop. A command
failure in a `while` body stops execution, including when the loop is used as a condition.

Function names and parameter declarations use bare names. Read parameter and
local variable values with `$name`. Each function call creates a fresh call scope;
`let` in the body stays in that scope, and repeating `let` in the same scope is an
error. Functions capture their defining lexical scope and return the last expression's value. Argument expressions are evaluated
before parameters are bound. Functions can return numbers, lists, maps, and booleans as well as
command output.
When called as a value, stdout is captured throughout the function body: if the
last expression produces a command status, captured stdout is returned as text;
otherwise the last typed value is returned. Earlier captured output is not
printed in that case.

## Function values and parameters

`fn` creates an anonymous function; `function` explicitly obtains a named user
function or ordinary builtin as a value. Functions can be stored in collections,
passed, returned, and called through expressions:

```parsh
(let inc (fn (x) (+ $x #1)))
($inc #2)
((fn (x) (* $x #2)) #3)
(let add (function +))
(apply $add #1 [#2 #3])

(defn sum (first &optional (second #0) &rest more)
  (let result (+ $first $second))
  (for n $more (set result (+ $result $n)))
  (return $result))
(sum #1 #2 #3 #4)
```

Parameters appear in this order: required, `&optional`, then `&rest name`.
An optional bare name defaults to nil. A `(name default)` expression runs only
when that argument is omitted, once per call, and can read preceding parameters
and captured bindings. Optional parameters consume positional arguments first;
remaining arguments form the rest list. A list argument remains one value.
`apply` expands its final list into arguments, optionally preceded by fixed
arguments. Values are never parsed or evaluated again.

Ordinary data retains value semantics; copies of a function retain its identity
and captured environment. `eq` compares function identity. A reference retains
the definition it obtained, even after that name is redefined. Closures retain
the defining lexical scope and observe updates made by outer `set` calls.
Functions display as `<function name>` and cannot implicitly become text,
external arguments, or environment values. `function` rejects macros, external
commands, and special forms such as `if`, `and`, and `let`. Ordinary builtins
include `get`, `not`, numeric and collection operations, `glob`, `cd`, and `type`.

`(return)` returns nil early; `(return expr)` evaluates once and exits the current
function through loops, nested expressions, and redirections. It does not exit
the caller. Using return outside a function is an error. Returned command
expressions use the usual value-position output capture rules. A macro body can
also return its expansion early; return in expanded code belongs to the caller.

## See the return value

```parsh
(defn greet (who)
  (str "hello, " $who))
(greet parsh)
(if #true (list yes) (list no))
(for item (list first last) (str $item))
```

An interactive session displays `"hello, parsh"`, `("yes")`, and `"last"`. A loop returns the last body's result, rather than collecting every iteration. These examples work in the browser playground.


`apply` also accepts external references from `command` and ordinary text names,
including quoted names such as `"str/join"`. Error handling uses
`try/catch/finally` and `throw`; see [execution results and tasks](/parsh/en/docs/execution/).
