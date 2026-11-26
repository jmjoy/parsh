---
title: "Functions & control flow"
description: "Lexical functions, branching, sequencing, and iteration."
---

```parsh
(defn greet (name)
  (echo hello $name))

(dump (if (test -d crates/parsh/src)
    (greet developer)
    (echo missing)))

(export EDITOR vim)
(dump (if (eq $EDITOR vim)
    (echo using vim)))

(dump (if (not (test -f config.txt))
    (echo configuration missing)))

(dump (do
  (let message "multiple expressions")
  (echo $message)))
```

`if` evaluates only the selected branch. A condition must produce a boolean
(from `eq`, a numeric comparison, or `not`) or a `CallResult`; a result is true when its `error` field is `#nil`. Text and
numbers are not implicitly booleans. Every parenthesized expression creates a fresh
lexical scope, so a `let` in a branch does not leak outside the `if`. `do` creates
a scope and runs its body expressions in sequence; bindings declared directly in
its body stay in that scope. `for` evaluates its list outside the loop, then creates
a fresh lexical scope and loop binding for each iteration.

`while` checks its condition before each iteration. Both loop forms accept one or
more body expressions and create a fresh scope per iteration: `let` stays local,
and `set` can update an outer binding.

`and/or` short-circuit from left to right and return booleans. `(and)` is true;
`(or)` is false. They accept the same conditions as `if`, including `CallResult`.

```parsh
(let count #0)
(dump (while (< $count #5)
  (set count (+ $count #1))
  (if (= $count #2) (continue))
  (if (= $count #4) (break [stopped $count]))
  (echo iteration $count)))
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
`let` in the body stays in that scope, and repeating `let` creates a new binding in
the same scope. Functions capture the bindings visible in their defining lexical
scope and return the last expression's value. Argument expressions are evaluated
before parameters are bound. Functions can return numbers, lists, maps, booleans, and `CallResult`.
Commands inside functions write to their current destinations in every calling
position. A final command returns its `CallResult` directly. Capture function
output explicitly with `(<-out (function-name args...))`.

## Function values and parameters

`fn` creates an anonymous function; `function` explicitly obtains a named user
function or ordinary builtin as a value. Functions can be stored in collections,
passed, returned, and called through expressions:

```parsh
(let inc (fn (x) (+ $x #1)))
(dump ($inc #2))
(dump ((fn (x) (* $x #2)) #3))
(let add (function +))
(dump (apply $add #1 [#2 #3]))

(defn sum (first &opt (second #0) &rest more)
  (let result (+ $first $second))
  (for n $more (set result (+ $result $n)))
  (return $result))
(dump (sum #1 #2 #3 #4))
```

Parameters appear in this order: required, `&opt`, then `&rest name`.
An optional bare name defaults to nil. A `(name default)` expression runs only
when that argument is omitted, once per call, and can read preceding parameters
and captured bindings. Optional parameters consume positional arguments first;
remaining arguments form the rest list. A list argument remains one value.

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
expressions stream output and return `CallResult`; text capture is explicit. A macro body can
also return its expansion early; return in expanded code belongs to the caller.

## Keyword arguments

Use `&name expression` to pass an argument by parameter name. Required and
optional parameters both accept keywords. Put positional arguments first, then
write the keywords in any order. `&rest` collects extra positional values.

```parsh
(defn greet (name &opt (greeting Hello) (punctuation "!"))
  (str $greeting ", " $name $punctuation))

(dump (greet parsh &punctuation "."))
(dump (greet &greeting Hi &name parsh))
(dump (apply greet [parsh] &greeting Hi))
```

Direct calls check for unknown keywords, repeated bindings, and missing
arguments before evaluating the arguments once in written order. Omitted
optional parameters use their defaults. An explicit `#nil` supplies a value.

An unquoted keyword name follows the same rules as a variable name. To pass the
text `&greeting`, use `"&greeting"` or a variable holding that text.
With `apply`, put fixed positional arguments and the list to expand before
any keyword pairs. The list supplies positional values, and `apply` forwards
the keyword pairs to the target. Aliases and `partial` can preset keyword
arguments for a target that accepts them.

## See the return value

```parsh
(defn greet (who)
  (str "hello, " $who))
(dump (greet parsh))
(dump (if #true [yes] [no]))
(dump (for item [first last] (str $item)))
```

An interactive session displays `"hello, parsh"`, `("yes")`, and `"last"`. A loop returns the last body's result, rather than collecting every iteration. These examples work in the browser playground.

`apply` also accepts external references from `command` and ordinary text names,
including quoted names such as `"str/join"`. Error handling uses
`try/catch/finally` and `throw`; see [execution results and tasks](/parsh/en/docs/execution/).
