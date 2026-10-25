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
(let point (Point 3 4))
(echo (Point.x $point) (Point.y $point))
(if (Point? $point) (echo a point))
```

A struct is an ordinary value: it can sit in lists and maps, be passed to
functions, and be captured by interpolation. It prints in field order, for
example `(Point 3 4)`, or `(Point "3" "4")` when the fields are text. It
compares by name and fields, and copies with the usual value semantics. Structs are immutable, so changing a field means building a new value.

Accessors read the field from the value rather than from the current definition,
so redefining a struct keeps existing values readable:

```parsh
(defstruct Point x y)
(let old (Point 1 2))
(defstruct Point x)
(Point.y $old)
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

(unless (test -f config.txt)
  (echo "configuration missing"))
```

Macro parameters are variables like any other, so they are read with `$name`
inside the body. Because the returned form is evaluated at the call site, a
template that mentions a variable refers to the caller's binding:

```parsh
(defmacro greet (who)
  `(echo hello ,$who))
(let who parsh)
(greet $who)
```

`quote` builds a form without evaluating it. Forms behave like values: `eq`
compares them by syntax, and the interactive prompt prints them as source.

```parsh
(quote (a b))
(eq (quote (a b)) (quote (a   b)))
(defmacro begin (body) `(do ,@$body))
(begin ((echo one) (echo two)))
```

Unquote accepts a quoted form or a scalar value. Splicing accepts a quoted list
form or a list of quoted forms.

## Limits

- Macro parameters use the same fixed arity as `defn`; there is no rest argument.
- Macros and structs are defined globally, even inside a function body.
- There is no hygiene: templates insert forms verbatim into the caller scope.
- Macros and struct members cannot be used as pipeline stages.
- A nested quasiquote is copied literally rather than nested one level deeper.
- Structs have no field setters and no dynamic field names.
