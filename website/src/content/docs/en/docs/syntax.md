---
title: "Syntax & values"
description: "Words, strings, typed values, and structured comments."
---

Every call is a parenthesized expression. Inner calls are evaluated as arguments:

```parsh
(str "hello, " (str "a " "friend"))
(list one "two words" (eq a a))
```

In value positions, bare words are text: `123`, `true`, `false`, and `nil` have no
special literal meaning. Double-quoted strings support `\"`, `\\`, `\n`, `\r`, and `\t`.
Strings do not interpolate variables. Variable references use `$name`. Binding
and assignment targets use bare names, including function parameters, loop
bindings, and environment settings.

The reader creates text, variable references, and lists of expressions. Evaluation can return text, booleans, lists, nil, or native command statuses. These are different value types even though words such as `true` and `nil` themselves are just text.

```parsh
(eq true true)
(not (eq a b))
(list a "b c" (eq x x))
```

The interactive results are `true`, `true`, and `("a" "b c" true)`. Text is displayed with quotes and escapes; nil is not displayed. A file or `-c` script runs expressions without automatically printing their results.

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
