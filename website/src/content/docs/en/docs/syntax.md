---
title: "Syntax & values"
description: "Words, strings, value types, and structured comments."
---

Every call is a parenthesized expression. Inner calls are evaluated as arguments:

```parsh
(str "hello, " (str "a " "friend"))
(list one "two words" (eq a a))
```

In value positions, bare words are text: `123`, `true`, `false`, and `nil` have no
special literal meaning. Double-quoted strings interpolate `$name` and `$(...)`,
and support `\"`, `\\`, `\n`, `\r`, `\t`, and `\$`. Single-quoted strings preserve
their content without interpolation or escapes. `${name}` remains literal text.
Outside strings, variable references use `$name`. Binding
and assignment targets use bare names, including function parameters, loop
bindings, and environment settings.

The reader creates text, variable references, interpolated strings, and lists of expressions. Evaluation can return text, numbers, booleans, lists, nil, or native command statuses. These are different value types even though words such as `true` and `nil` themselves are just text.

```parsh
(eq true true)
(not (eq a b))
(list a "b c" (eq x x))
```

The interactive results are `true`, `true`, and `("a" "b c" true)`. Text is displayed with quotes and escapes; nil is not displayed. A file or `-c` script runs expressions without automatically printing their results.

## Strings and interpolation

```parsh
(let name parsh)
(str "hello, $name")
(str "$(get name)_suffix")
(str "sum=$(+ 1 2)")
(str "nested=$(str "a" 'b')")
(str "cwd=$(pwd)")
(str '$name $(+ 1 2)\n')
(str "\$name \$(+ 1 2)")
```

`$name` reads the longest valid variable name. Use `$(get name)` when text such
as `_suffix` follows the name. `${name}` has no special meaning, and a `$` that
does not start a variable name or `$(` remains text.

The parentheses in `$(...)` are an ordinary call expression. `$(name)` invokes
the zero-argument function or command `name`; `$(get name)` reads the variable.
All existing expressions are supported, including nested calls, control flow,
and structured comments. Inner strings use normal parsh syntax without extra
escaping for the outer string. The `pwd` example needs a command execution host.

Parts evaluate once from left to right, in the current scope, with the same
effects and errors as ordinary calls. Scalars convert as `str` does: numbers and
booleans become text, nil becomes empty text, and lists are errors. External
commands capture UTF-8 stdout and remove trailing LF characters; stderr keeps
its destination. Errors stop the current batch. The resulting text stays one
argument, and inserted values are never parsed or interpolated again.

Single quotes preserve everything until the next single quote, including
backslashes, dollar signs, and newlines. Use double quotes to include a single
quote character. Adjacent quoted and bare expressions do not concatenate.
To pass a literal `$name` or `$(...)` to another program such as `sh -c`, use
single quotes or escape the dollar with `\$`. Text display also escapes dollars
so copying a displayed string does not trigger interpolation.

## Arithmetic and numeric comparisons

`123` is text until a numeric operation parses it. Arithmetic operators `+`, `-`,
`*`, `/` and comparison operators `=`, `!=`, `<`, `<=`, `>`, `>=` accept runtime
numbers or numeric text. Variables and captured command output
follow the same rule as bare words and quoted strings.

| Expression | Interactive result |
| --- | --- |
| `(+ 123 2)` | `125` |
| `(+ "1.5" 2)` | `3.5` |
| `(/ 5 2)` | `2.5` |
| `(/ 8 2)` | `4.0` |
| `(str 00123)` | `"00123"` |
| `(eq (+ 1 2) 3)` | `false` |
| `(eq (+ 3) (+ 3.0))` | `false` |
| `(= (+ 1 2) 3)` | `true` |
| `(= 001 1.0)` | `true` |
| `(!= 1 1.5)` | `true` |
| `(< 2 10)` | `true` |
| `(<= 3 3.0)` | `true` |
| `(> 10 2)` | `true` |
| `(>= 2.5 3)` | `false` |

Text is trimmed and must parse completely. Supported forms include `+001`,
`-12`, `.5`, `1.`, and `2.5e1`. Integers use `i64`; a decimal point or exponent
selects `f64`. Leading zeroes are decimal, not octal. Out-of-range integer text
is an error rather than being converted to a float. Booleans, nil, and lists
cannot be used as numbers.

Arguments are evaluated once from left to right; arithmetic operations fold left.
`+` and `*` accept any number of arguments, with `(+)` returning integer `0`
and `(*)` returning integer `1`. `-` and `/` need at least one argument;
`(- 5)` returns `-5` and `(/ 4)` returns `0.25`. Integer addition, subtraction,
and multiplication check for overflow. Each step involving a float uses `f64`;
division always returns a float. A later float does not prevent an earlier
integer step from overflowing.

`eq` compares types and values, including elements inside lists. Numeric `=`
tests equality and `!=` tests inequality; `<`, `<=`, `>`, and `>=` test order.
Each numeric comparison takes exactly two arguments and returns a boolean.
Comparisons accept numeric text and compare integers and floats by value,
preserving integer precision even for large mixed operands:
`(> 9007199254740993 9007199254740992.0)` is `true`.
Float comparisons are exact, with no tolerance: `(= (+ 0.1 0.2) 0.3)` is
`false` because `f64` has binary rounding error. Arithmetic involving floats
can also lose large integer precision.

```parsh
(let count (+ 1 2))
(if (>= $count 3) (str "at least three") (str fewer))
```

Numbers display without quotes. Floats retain a decimal point or exponent,
including `4.0` and `-0.0`; `str`, external command arguments, and environment
values use the same format. Capturing a command or exporting a number produces
text again. Numbers are not implicitly booleans; use a numeric comparison in a condition.
Invalid operands, division by zero, integer overflow, and non-finite floats
report a language error at the failing operand with exit status `2`.

## Structured comments

```parsh
(; This is a whole comment expression.
   (; Comments can nest. )
   (no-such-command $undefined)
)

(echo (; This comment contributes no argument. ) hello)
(echo ; this;is;text)
```

A list whose first word is `;` is discarded by the reader, wherever it occurs.
Comment contents still follow the string and balanced-parenthesis rules, but
are never evaluated or checked for defined names. Elsewhere, `;` is ordinary
text; there are no line comments. A quoted `";"` is not a comment marker.
