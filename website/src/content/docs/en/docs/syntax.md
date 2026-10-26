---
title: "Syntax & values"
description: "Words, strings, value types, and structured comments."
---

Every call is a parenthesized expression. Inner calls are evaluated as arguments:

```parsh
(str "hello, " (str "a " "friend"))
(list one "two words" #true)
```

In value positions, bare words are text: `123`, `true`, `false`, and `nil` have no
special literal meaning. Double-quoted strings interpolate `$name` and `$(...)`,
and support `\"`, `\\`, `\n`, `\r`, `\t`, and `\$`. Single-quoted strings preserve
their content without interpolation or escapes. `${name}` remains literal text.
Outside strings, variable references use `$name`. Binding
and assignment targets use bare names, including function parameters, loop
bindings, and environment settings.

The reader creates text, scalar literals, variable references, interpolated strings, and lists of expressions. Evaluation can return text, numbers, booleans, lists, maps, nil, or native command statuses. These are different value types even though words such as `true` and `nil` themselves are just text.

```parsh
(eq true true)
(not #false)
(list a "b c" #true)
```

The interactive results are `#true`, `#true`, and `("a" "b c" #true)`. Text is displayed with quotes and escapes; nil is not displayed. A file or `-c` script runs expressions without automatically printing their results.

`(map name parsh)` constructs a map and displays as `(map "name" "parsh")`.
Maps have text keys and preserve insertion order. See [maps](/parsh/en/docs/variables/#maps)
for construction, lookup, and update commands.

## Scalar literals

Use `#` to create typed scalars: `#1` is an int, `#1.0` and `#1e3` are floats,
`#true` and `#false` are booleans, and `#nil` is nil. Plain `1`, `false`, and
`nil` remain text. For example:

```parsh
(let config {retries #3 timeout #1.5 enabled #false})
(list (typeof #1) (typeof 1) (eq #1 1))
```

The list displays as `("int" "text" #false)`. Numeric and boolean results display
with `#`, including inside collections; nested nil displays as `#nil`. A top-level
nil result stays silent. `str`, interpolation, external arguments, and environment
values use ordinary text without the prefix: `(str #1 #false)` returns `"1false"`.

All unquoted words beginning with `#` are reserved. Invalid forms such as `#flase`,
`#123abc`, or a lone `#` are reader errors. Quote text such as `"#fff"`; `abc#1`
remains ordinary text. A shebang is allowed at the very start of source. Structured
comments do not validate `#` literals, but still check strings and balanced delimiters.
Literal spelling must be contiguous. Numbers use the existing signed decimal,
fraction, and exponent syntax, with i64 integers and finite f64 floats; integer
overflow never falls back to floating point. Boolean and nil names are lowercase,
with no `#t`/`#f` aliases.

Literals can appear directly at the prompt or at script top level. Scripts do not
print their results automatically. A literal is source syntax: quoted text,
variable contents, and command output are never parsed again, and `#$name` is not
a conversion. Use `int`, `float`, or `bool` for runtime conversion.

A command head must evaluate to text or a function. `(#1)` and a numeric variable
used as a command name are errors; use `str` to explicitly construct a text command
name. `#false` is an ordinary false value, while `(false)` retains its external
command failure behavior. Numeric values and nil are not implicit conditions.

## Collection shorthand

`[...]` is shorthand for `(list ...)`, and `{...}` is shorthand for `(map ...)`:

```parsh
[]
[one "two words" #true]
{}
{name parsh items [one two] nested {enabled #true}}
(let items [a (; ignored) b])
```

Elements are separated by whitespace. Maps take alternating keys and values,
with the same text-key and duplicate-key rules as `map`. Elements, keys, and values
evaluate once from left to right. Collections can nest and appear anywhere a call can.
Interactive results keep their existing format: `("one" "two words" #true)` and `(map ...)`.

All three delimiter pairs, `()`, `[]`, and `{}`, must match, including inside
structured comments. Only `(; ...)` introduces a comment; `[; a]` contains two text values.
Outside strings, brackets and braces delimit expressions even without whitespace.
Quote literal text and glob patterns containing them, for example `(glob 'file[12].txt')`.
Brackets and braces inside strings are ordinary text, and `${name}` remains literal text.

At the prompt, type or paste a collection directly. Enter can supply one final
matching `]` or `}` when that completes the entire input; unfinished nested structures
continue on a new line. The gray closing delimiter is only a display hint until submission.

## Strings and interpolation

```parsh
(let name parsh)
(str "hello, $name")
(str "$(get name)_suffix")
(str "sum=$(+ #1 #2)")
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

Parts evaluate once from left to right. Each parenthesized part has its own
lexical scope, while `let` binds in the surrounding expression scope; side effects
and errors otherwise match ordinary calls. Scalars convert as `str` does: numbers and
booleans become text, nil becomes empty text, and lists and maps are errors. External
commands capture UTF-8 stdout and remove trailing LF characters; stderr keeps
its destination. Errors stop the current batch. The resulting text stays one
argument, and inserted values are never parsed or interpolated again.

Ordinary single quotes preserve everything until the next single quote, including
backslashes, dollar signs, and newlines. Use double quotes to include a single
quote character. Adjacent quoted and bare expressions do not concatenate.
To pass a literal `$name` or `$(...)` to another program such as `sh -c`, use
single quotes or escape the dollar with `\$`. Text display also escapes dollars
so copying a displayed string does not trigger interpolation.

Triple quotes let strings contain individual quotes or pairs of quotes of the
same kind without escaping them. `"""..."""` supports the same interpolation and
escapes as ordinary double quotes; `'''...'''` preserves all content literally:

```parsh
(str """hello, "$name"
sum=$(+ #1 #2)""")
(str '''literal '$name'
$(pwd)\n''')
```

All newlines, indentation, and leading or trailing whitespace are preserved,
including a newline immediately after the opening delimiter. Only three matching
quotes end the string. In triple double quotes, `\"` escapes a quote, so `\"""`
inserts three literal double quotes. Six consecutive matching quotes form an
empty string. Adjacent strings remain separate expressions.

## Arithmetic and numeric comparisons

`123` is text, while `#123` is an integer. Arithmetic operators `+`, `-`, `*`,
`/` and numeric comparisons `=`, `!=`, `<`, `<=`, `>`, `>=` accept only runtime
int or float values, including mixed numeric types. Text is never parsed by an
operator: `(+ 1 #2)` is a type error; `(+ (int 1) #2)` returns `#3`.
Convert environment variables, script arguments, and captured command output
explicitly with `int` or `float` before using them as numbers.

| Expression | Interactive result |
| --- | --- |
| `(+ #123 #2)` | `#125` |
| `(+ (float "1.5") #2)` | `#3.5` |
| `(/ #5 #2)` | `#2.5` |
| `(/ #8 #2)` | `#4.0` |
| `(str 00123)` | `"00123"` |
| `(eq (+ #1 #2) 3)` | `#false` |
| `(eq #3 #3.0)` | `#false` |
| `(= (+ #1 #2) #3)` | `#true` |
| `(= #001 #1.0)` | `#true` |
| `(!= #1 #1.5)` | `#true` |
| `(< #2 #10)` | `#true` |
| `(<= #3 #3.0)` | `#true` |
| `(> #10 #2)` | `#true` |
| `(>= #2.5 #3)` | `#false` |

Literal forms include `#+001`, `#-12`, `#.5`, `#1.`, and `#2.5e1`. Integers
use `i64`; a decimal point or exponent selects `f64`. Leading zeroes are decimal,
not octal. Out-of-range integer literals are errors, not floats. For runtime
text, `int` and `float` trim and parse completely; numeric operators do not.
Booleans, nil, lists, and maps cannot be used as numbers.

Arguments are evaluated once from left to right; arithmetic operations fold left.
`+` and `*` accept any number of arguments, with `(+)` returning integer `0`
and `(*)` returning integer `1`. `-` and `/` need at least one argument;
`(- #5)` returns `-5` and `(/ #4)` returns `0.25`. Integer addition, subtraction,
and multiplication check for overflow. Each step involving a float uses `f64`;
division always returns a float. A later float does not prevent an earlier
integer step from overflowing.

`eq` compares types and values, including elements inside lists and maps; map insertion order does not affect equality. Numeric `=`
tests equality and `!=` tests inequality; `<`, `<=`, `>`, and `>=` test order.
Each numeric comparison takes exactly two arguments and returns a boolean.
Comparisons require int or float values and compare them by value,
preserving integer precision even for large mixed operands:
`(> #9007199254740993 #9007199254740992.0)` is `true`.
Float comparisons are exact, with no tolerance: `(= (+ #0.1 #0.2) #0.3)` is
`false` because `f64` has binary rounding error. Arithmetic involving floats
can also lose large integer precision.

```parsh
(let count (+ #1 #2))
(if (>= $count #3) (str "at least three") (str fewer))
```

Numbers display with `#`, and floats retain a decimal point or exponent,
including `#4.0` and `#-0.0`. `str`, external command arguments, and environment
values use numeric text without `#`. Capturing a command or exporting a number produces
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

See the [standard library](/parsh/en/docs/stdlib/) for list, text, type, and numeric helpers.
