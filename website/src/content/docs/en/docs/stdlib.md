---
title: "Standard library"
description: "Immutable lists, Unicode text, type predicates, and numeric helpers."
---

These builtins share their implementation between native parsh and the browser.
They support `(function name)` and `apply`. Collection updates return new values;
use `set` to replace a binding. In the signatures below, brackets mark optional
arguments and `...` marks repeated arguments.

## Lists

| Call | Result |
| --- | --- |
| `(llen xs)` | Integer length |
| `(lfirst xs [default])` | First item; default or error on an empty list |
| `(lrest xs)` | A new list without the first item; empty stays empty |
| `(lget xs index [default])` | Indexed item; default or error out of bounds |
| `(lappend xs value...)` | A new list with each value appended, preserving nested lists |
| `(lconcat xs...)` | Concatenated lists; no arguments returns an empty list |
| `(lslice xs start [end])` | Slice excluding end; omitted end means the list's end |

```parsh
(let original [a b])
(let updated (lappend $original [c d]))
(list $original $updated)
(lconcat $original [c d])
(lget $updated -1)
(lslice $updated 1 -1)
```

Indices start at zero. Negative indices count from the end; `-1` is the last
item. Indices accept integers or integer text, not floats. Item reads outside
the list require a default or raise an error. Slice bounds are clamped, and
reversed ranges return an empty list.

## Text

| Call | Result |
| --- | --- |
| `(slen text)` | Integer length using Rust `chars()` |
| `(sget text index [default])` | One character as text; default or error out of bounds |
| `(sslice text start [end])` | Slice using the same character unit |
| `(ssplit text separator)` | List of text fields split on a literal separator |
| `(sjoin texts separator)` | A text list joined with the separator |
| `(slines text)` | Lines without LF or CRLF; a final newline adds no empty line |
| `(strim text)` | Text with leading and trailing Unicode whitespace removed |
| `(sreplace text from to)` | All non-overlapping literal matches replaced with to |
| `(scontains text part)` | Boolean indicating a literal substring match |

```parsh
(list (slen "中文") (sget "中文" -1) (sslice "中文abc" 1 3))
(sjoin (ssplit "a,b,c" ",") " / ")
(slines "a\r\nb\n")
```

Text shares list indexing, slicing, negative-index, and default rules. The unit
is a Unicode scalar value: combining accents and emoji sequences may count as
several characters. Text arguments must be text; `sjoin` requires text elements.
Use `str` for explicit conversions. Splitting preserves interior and trailing
empty fields. An empty separator follows Rust `split("")`, including empty end
fields. Replacement follows Rust `replace`; an empty pattern inserts text
between characters and at both ends.

## Explicit defaults are eager

Defaults passed to `mget`, `lget`, `lfirst`, and `sget` are ordinary arguments.
After checking arity, arguments run once from left to right before looking up
the result. Even a successful lookup executes the default's side effects and
propagates its errors:

```parsh
(let calls (+))
(mget {name parsh} name (do (set calls (+ $calls 1)) fallback))
(list $calls)
```

Results are `"parsh"` and `(1)`. A stored nil value counts as present and returns
nil, but the explicit default still runs. Function references and `apply` follow
the same rule. Defaults declared with `&optional` run only when an argument is omitted.

## Type predicates

`typeof` returns a type name as text: `text`, `int`, `float`, `bool`, `list`,
`map`, `form`, `struct`, `function`, `nil`, or `status`. Corresponding predicates
are `text?`, `int?`, `float?`, `bool?`, `list?`, `map?`, `form?`, `struct?`,
`function?`, `nil?`, and `status?`. `number?` accepts both integers and floats.
`empty?` accepts text, lists, and maps, and rejects other types.

Predicates inspect runtime types strictly: `(number? 123)` is false because the
bare word is text; `(number? (+ 123))` is true. Commands in value position are
captured as text. `type` continues to inspect command lookup.

## Numeric helpers

| Call | Result |
| --- | --- |
| `(quot a b)` | Integer quotient truncated toward zero |
| `(rem a b)` | Corresponding remainder, with the dividend's sign |
| `(abs n)` | Absolute value, retaining integer or float type |
| `(min n...)`, `(max n...)` | At least one operand; winning number, first operand on a tie |
| `(floor n)` | i64 rounded down |
| `(ceil n)` | i64 rounded up |
| `(round n)` | Nearest i64, with ties away from zero |

```parsh
(list (quot -7 3) (rem -7 3) (round 2.5) (round -2.5))
```

The result is `(-2 -1 3 -3)`. `quot/rem` require integers or integer text; other
numeric helpers accept numbers or numeric text. `min/max` retain the winning
number's type and preserve integer precision in mixed comparisons. Division by
zero, integer overflow, rounding outside i64, and non-finite values are language errors.
