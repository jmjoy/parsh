---
title: "Standard library"
description: "Immutable lists, Unicode text, type predicates, and numeric helpers."
---

These builtins share their implementation between native parsh and the browser.
They support `(function name)` and `apply`. Collection updates return new values;
use `set` to replace a binding. In the signatures below, brackets mark optional
arguments and `...` marks repeated arguments.

## Module namespaces

Text, list, and map operations use `module/member` names, such as
`str/join`, `list/append`, and `map/get`. These are ordinary builtin functions:

```parsh
(str/join [a b] ",")
(list/append [a] b)
(map/get {name parsh} name)
(apply (function str/join) [[a b] ","])
```

The constructors `str`, `list`, and `map` keep their names. The old prefixed
operation names have been removed; scripts must use the module names.
Builtin namespaces are available directly, without imports. User module
declarations and file imports are not yet supported.

## Scalar conversions

Each conversion takes exactly one argument, checks arity before evaluation, and
evaluates that argument once. Function references and `apply` use the same rules.
`int`, `float`, and `bool` are reserved ordinary builtin names.

| Call | Accepted input and result |
| --- | --- |
| `(int value)` | An int, an integral float in i64 range, or numeric text parsed by the existing number rules. `(int "1e2")` returns `#100`; `(int #1.5)` is an error. |
| `(float value)` | An int, float, or numeric text parsed directly as f64. Normal rounding is allowed, including for large integers; non-finite results are errors. |
| `(bool value)` | A boolean, or text that is exactly lowercase `true` or `false` after trimming. Numbers, nil, and other types are errors. |

Numeric text is trimmed and must parse completely. For `int`, text containing a
point or exponent is first parsed as f64, so integrality is checked after f64
rounding. Integer text must fit in i64; `(float "9223372036854775808")` can instead
parse directly as a finite float. Booleans are not numbers, and no conversion
accepts collections, forms, functions, nil, or command statuses. Text `"#1"` and
`"#false"` is not re-read as literal syntax. Errors have exit status `2` and point
to the operand.

```parsh
(int " 1.0 ")
(float #1)
(bool " false ")
(apply (function int) ["1e2"])
```

Results are `#1`, `#1.0`, `#false`, and `#100`.

## Lists

| Call | Result |
| --- | --- |
| `(list/len xs)` | Integer length |
| `(list/first xs [default])` | First item; default or error on an empty list |
| `(list/rest xs)` | A new list without the first item; empty stays empty |
| `(list/get xs index [default])` | Indexed item; default or error out of bounds |
| `(list/append xs value...)` | A new list with each value appended, preserving nested lists |
| `(list/concat xs...)` | Concatenated lists; no arguments returns an empty list |
| `(list/slice xs start [end])` | Slice excluding end; omitted end means the list's end |

```parsh
(let original [a b])
(let updated (list/append $original [c d]))
(list $original $updated)
(list/concat $original [c d])
(list/get $updated #-1)
(list/slice $updated #1 #-1)
```

Indices start at zero. Negative indices count from the end; `#-1` is the last
item. Indices and slice bounds require int values. Text and integral floats are errors; use `int` explicitly when needed. Item reads outside
the list require a default or raise an error. Slice bounds are clamped, and
reversed ranges return an empty list.

## Text

| Call | Result |
| --- | --- |
| `(str/len text)` | Integer length using Rust `chars()` |
| `(str/get text index [default])` | One character as text; default or error out of bounds |
| `(str/slice text start [end])` | Slice using the same character unit |
| `(str/split text separator)` | List of text fields split on a literal separator |
| `(str/join texts separator)` | A text list joined with the separator |
| `(str/lines text)` | Lines without LF or CRLF; a final newline adds no empty line |
| `(str/trim text)` | Text with leading and trailing Unicode whitespace removed |
| `(str/replace text from to)` | All non-overlapping literal matches replaced with to |
| `(str/contains text part)` | Boolean indicating a literal substring match |

```parsh
(list (str/len "中文") (str/get "中文" #-1) (str/slice "中文abc" #1 #3))
(str/join (str/split "a,b,c" ",") " / ")
(str/lines "a\r\nb\n")
```

Text shares list indexing, slicing, negative-index, and default rules. The unit
is a Unicode scalar value: combining accents and emoji sequences may count as
several characters. Text arguments must be text; `str/join` requires text elements.
Use `str` for explicit conversions. Splitting preserves interior and trailing
empty fields. An empty separator follows Rust `split("")`, including empty end
fields. Replacement follows Rust `replace`; an empty pattern inserts text
between characters and at both ends.

## Explicit defaults are eager

Defaults passed to `map/get`, `list/get`, `list/first`, and `str/get` are ordinary arguments.
After checking arity, arguments run once from left to right before looking up
the result. Even a successful lookup executes the default's side effects and
propagates its errors:

```parsh
(let calls #0)
(map/get {name parsh} name (do (set calls (+ $calls #1)) fallback))
(list $calls)
```

Results are `"parsh"` and `(#1)`. A stored nil value counts as present and returns
nil, but the explicit default still runs. Function references and `apply` follow
the same rule. Defaults declared with `&optional` run only when an argument is omitted.

## Type predicates

`typeof` returns a type name as text: `text`, `int`, `float`, `bool`, `list`,
`map`, `form`, `struct`, `function`, `nil`, or `status`. Corresponding predicates
are `text?`, `int?`, `float?`, `bool?`, `list?`, `map?`, `form?`, `struct?`,
`function?`, `nil?`, and `status?`. `number?` accepts both integers and floats.
`empty?` accepts text, lists, and maps, and rejects other types.

Predicates inspect runtime types strictly: `(number? 123)` is false because the
bare word is text; `(number? #123)` is true. Commands in value position are
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
(list (quot #-7 #3) (rem #-7 #3) (round #2.5) (round #-2.5))
```

The result is `(#-2 #-1 #3 #-3)`. `quot/rem` require int values; other numeric helpers require int or float
values. Text must be converted explicitly with `int` or `float`. `min/max` retain the winning
number's type and preserve integer precision in mixed comparisons. Division by
zero, integer overflow, rounding outside i64, and non-finite values are language errors.


## Calls, results, and tasks

`command`, `command?`, `callable?`, `bytes?`, `capture-call`, `stdout-text`, `stderr-text`, `exit-code`, `signal`, `check`, and `throw` work in native and browser sessions. `spawn`, `wait`, `job-status`, and `cancel` require native task support. All are ordinary builtins available through `function` and `apply`. See [execution results and tasks](/parsh/en/docs/execution/) for signatures and result structures.
