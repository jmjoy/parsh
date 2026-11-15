---
title: "Structs & macros"
description: "Named records, quoted forms, and user-defined commands."
---

Two forms extend the language itself: `defstruct` declares a record type, and
`defmacro` defines a command that receives its arguments as quoted forms.

## Structs

`(defstruct Name field...)` defines a struct with at least one field and creates
three kinds of command:

| Name | Meaning |
| --- | --- |
| `Name` | Constructor: one argument per field, in definition order |
| `Name.field` | Accessor: reads one field from a value of that struct |
| `Name?` | Predicate: true when the value is that struct |

```parsh
(defstruct Point x y)
(let point (Point #3 #4))
(echo (. $point x) (. $point y))
(dump (if (Point? $point) (echo a point)))
```

`(. value field...)` reads one or more fields from any struct without naming its type.
Each field name is ordinary text, so bare words, strings, variables, and expressions
all work. Multiple fields are chained from left to right, and the existing
`(Point.x $point)` accessor remains available:

```parsh
(let field x)
(dump (. $point "x"))
(dump (. $point $field))
(dump (. $point (str x)))
(dump (apply (function .) [$point y]))
```

`.` takes at least two arguments and returns the final field's original value and type.
`(. $point x y)` is equivalent to `(. (. $point x) y)`. It checks arity before
evaluation, then evaluates the root and each reached field argument once from left to right,
stopping at the first error.
A non-struct intermediate value, a non-text field name, or a missing field raises
an error. Read nested structs directly with `(. $segment end x)`.

A struct can sit in lists and maps and be passed to functions. It displays in
field order, for example `(Point #3 #4)`, or `(Point "3" "4")` when the fields are
text. It compares by name and fields and uses ordinary data value semantics.
Structs are immutable, so changing a field means building a new value.

Structs cannot implicitly become text, external arguments, or environment values.
Read scalar fields for interpolation, use `dump` for display, or encode as JSON:

```parsh
(dump $point)
(dump (str "x=$(. $point x)"))
(dump (json/encode $point))
```

Accessors read the field from the value rather than from the current definition,
so redefining a struct keeps existing values readable:

```parsh
(defstruct Point x y)
(let old (Point #1 #2))
(defstruct Point x)
(dump (. $old y))
```

`type` reports struct members without calling them:

```parsh
(defstruct Point x y)
(type Point)
(type Point.x)
(type -t Point)
```

Names must not collide with built-ins, functions, or macros. The constructor,
accessors, and predicate are reserved for that struct.

## Macros

`(defmacro Name (parameters...) body...)` defines a command whose arguments are
not evaluated. Each parameter is bound to the quoted form of the corresponding
argument, the body runs, and the form it returns is evaluated in the caller's
scope.

Backquote starts a template. `,` inserts one evaluated expression into it, and
`,@` flattens a list form into the surrounding call:

```parsh
(defmacro unless (condition body)
  `(if (not ,$condition) ,$body))

(dump (unless (test -f config.txt)
  (echo "configuration missing")))
```

Macro parameters are variables like any other, so they are read with `$name`
inside the body. Because the returned form is evaluated at the call site, a
template that mentions a variable refers to the caller's binding:

```parsh
(defmacro greet (who)
  `(echo hello ,$who))
(let who parsh)
(dump (greet $who))
```

`quote` builds a form without evaluating it. Forms behave like values: `eq`
compares them by syntax, and `dump` prints them as source.

```parsh
(dump (quote (a b)))
(dump (eq (quote (a b)) (quote (a   b))))
(defmacro begin (body) `(do ,@$body))
(dump (begin ((echo one) (echo two))))
```

Unquote accepts a quoted form or a scalar value. Splicing accepts a quoted list
form or a list of quoted forms.

## Optional and rest macro parameters

```parsh
(defmacro when (condition &rest body)
  `(if ,$condition (do ,@$body)))
(dump (when #true (str first) (str last)))
```

Explicit macro arguments remain unevaluated forms. `&rest` collects a list of
forms; omitted `&optional` defaults run during macro execution and can read
preceding parameters. Bare optional parameters default to nil. Return in a macro
body returns its expansion; return, break, and continue in expanded code act on
the caller's function or loop.

## Limits

- Macro parameters support `&optional` and `&rest` like functions; arguments remain unevaluated forms.
- Macros and structs are defined globally, even inside a function body.
- There is no hygiene: templates insert forms verbatim into the caller scope.
- Macros and struct members cannot be used as pipeline stages.
- A nested quasiquote is copied literally rather than nested one level deeper.
- Structs have no field setters; use `.` to read a dynamically selected field.

Scalar literals retain their types through quote, quasiquote, and unquote. Form source uses `#` for typed scalars and normalizes numeric spelling: `#001` prints as `#1`. Literal nodes compare by type and value, ignoring source offsets.

`CallResult`, `Exited`, `Signaled`, `StageResult`, `Job`, `JobStatus`, `Error`, and
`HttpResponse` are runtime-owned structs. Their constructors cannot be called or
redefined; use their accessors and predicates to inspect results. See
[execution results](/parsh/en/docs/execution/#complete-results) and
[HTTP](/parsh/en/docs/stdlib/#http). Macro expansion is limited to 32 nested levels.
