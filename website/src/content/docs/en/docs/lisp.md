---
title: "Lisp helpers and code"
description: "Function composition, nested updates, standard macros, syntax forms, and loading scripts."
---

These helpers extend the [standard library](/parsh/en/docs/stdlib/). Ordinary
functions support `function` and `apply` in native and browser sessions. Updates
return new values, and explicit arguments are evaluated once from left to right
after checking arity. Macros receive unevaluated forms.

## Functions and sequences

| Call | Result |
| --- | --- |
| `(identity value)` | The original value, preserving its type |
| `(constantly value)` | A function that accepts any arguments and always returns value |
| `(partial f args...)` | A function that prepends the captured arguments when calling f |
| `(compose functions...)` | A function applying functions from right to left |
| `(complement predicate)` | A function negating the predicate's boolean result |

Adapters accept the same function values, command references, and text names as
`apply`. Names are resolved when the adapter is created, retaining the current
function definition. Captured data uses value semantics. Adapters are ordinary
closures and can be passed to background tasks. `constantly` still evaluates
arguments supplied to its returned function.

The rightmost function in `compose` receives all call arguments; each remaining
function receives one result. `(compose)` is the unary identity function.
`complement` requires a boolean result, including when wrapping an external
command; a `CallResult` is not a boolean predicate result.

```parsh
(let add_ten (partial (function +) #10))
(dump (list/map $add_ten [#1 #2 #3]))
(dump ((compose (fn (n) (* $n #2)) (function +)) #2 #3))
(dump (list/filter (complement even?) [#1 #2 #3 #4]))
```

Results are `(#11 #12 #13)`, `#10`, and `(#1 #3)`.

`even?` and `odd?` accept exactly one int. `zero?`, `positive?`, and `negative?`
accept one int or float. All return booleans and reject implicit text or boolean
conversion. Negative floating-point zero is zero, neither positive nor negative.

| Call | Result |
| --- | --- |
| `(list/flat-map f xs)` | Concatenate each callback's list result, flattening exactly one level |
| `(list/sort-by key-fn xs)` | Stable ascending sort using a key computed once per item |
| `(list/group-by key-fn xs)` | Map from text keys to lists of original items |
| `(list/take-while predicate xs)` | Prefix before the first false predicate result |
| `(list/drop-while predicate xs)` | Suffix starting at the first false predicate result |
| `(list/partition size xs)` | Consecutive lists of up to size items, retaining a short final group |

Transforms visit items from left to right and keep nested values intact.
`flat-map` requires list results. Sort keys must be all text or all finite
numbers; ints and floats can be mixed without losing integer comparison
precision. Equal keys preserve input order. `group-by` requires text keys and
preserves both first-seen group order and item order within groups.
`take-while` and `drop-while` require boolean predicates and stop calling them at
the first false result. Empty inputs produce an empty list or map. `partition`
requires a positive int size and supports sizes larger than the input length.

## Nested maps

| Call | Result |
| --- | --- |
| `(map/update m key f args...)` | New map with `f` called on the old value and extra arguments |
| `(map/get-in m path [default])` | Value at a nested path; default or error if missing |
| `(map/set-in m path value)` | New map with the value stored at path |
| `(map/update-in m path f args...)` | New map with `f` called on the nested old value and extra arguments |

Paths contain 1–128 text keys. Paths traverse maps; a present non-map
intermediate value is an error. `set-in` and `update-in` create missing
intermediate maps. Update callbacks run once, receive nil for a missing leaf,
and must handle nil explicitly when needed. Existing key positions are retained;
new keys are appended.

`get-in` treats an existing nil as a hit. Its explicit default is eager, just
like `map/get`, even for a successful lookup. A missing intermediate key can use
the default; an intermediate non-map value remains an error.

```parsh
(let original {http {retries #2}})
(let updated (map/update-in $original [http retries] (function +) #1))
(dump (list $original $updated))
(dump (map/get-in $updated [http retries]))
```

The original retains `#2` and the updated map contains `#3`.

## Standard macros

Every session starts with these ordinary, redefinable macros:

| Call | Behavior |
| --- | --- |
| `(when condition body...)` | Run body when condition succeeds |
| `(unless condition body...)` | Run body when condition fails |
| `(cond (condition body...) ... (else body...))` | Run the first matching clause; an optional else must be last |
| `(-> value steps...)` | Insert the preceding expression as the first argument of each step |
| `(->> value steps...)` | Insert the preceding expression as the last argument of each step |

Conditions use the existing boolean or `CallResult` rules. Bodies run only when
selected and return their last value. If no body is selected, the result is nil. Each `cond`
clause requires a condition and at least one body expression. A bare `else`
marks the default clause. Threading steps may be call forms or callable names;
the initial expression is evaluated once. No steps returns its value.

```parsh
(dump (cond ((< #3 #2) small) ((= #3 #3) equal) (else other)))
(dump (-> {http {retries #2}} (map/get-in [http retries]) (+ #1)))
(dump (->> [#1 #2 #3 #4] (list/filter even?) (list/map (partial + #10))))
```

Results are `"equal"`, `#3`, and `(#12 #14)`. Macros cannot be referenced with
`function` or invoked with `apply`. See [structs and macros](/parsh/en/docs/structs-and-macros/)
for custom definitions.

## Forms, reading, and evaluation

Forms are syntax values, distinct from runtime lists. Reader sugar such as
`[a b]` is represented as the call form `(list a b)`.

| Call | Result |
| --- | --- |
| `(form/kind form)` | `word`, `variable`, `text`, `int`, `float`, `bool`, `nil`, `list`, `interpolated`, or `comment` |
| `(form/value form)` | Atomic payload; words and variable names become text; list and interpolated forms are errors |
| `(form/children form)` | List of child forms from a list or interpolated form |
| `(form/list forms)` | A call form constructed from a runtime list of forms |
| `(form/word name)` | A word form from one bare word or an identifier form |
| `(form/variable name)` | A variable reference from a valid name or identifier form |
| `(form/from-value value)` | Literal syntax for text, numbers, booleans, nil, lists, and maps |
| `(form/source form)` | Source representation as text |
| `(read text)` | Exactly one form, without evaluating it; comments are ignored |
| `(eval form)` | Evaluate a form in the caller's surrounding expression scope |
| `(macroexpand-1 form)` | Expand one outer macro invocation without executing the resulting form |
| `(gensym [prefix])` | A fresh uninterned identifier form; prefix defaults to G |

`form/from-value` preserves literal data, including dollar signs inside text;
functions, commands, bytes, structs, and forms are not literal-data inputs.
`form/list` requires forms as children and limits nesting to 256 levels.
`form/from-value` limits nested literal data to 128 levels.

`read` rejects empty input, multiple forms, and malformed source with a catchable
error of kind `read`. Use raw strings when the source contains variables that
should be resolved during evaluation:

```parsh
(let n #10)
(dump (eval (read '(+ $n #2)')))
(dump (macroexpand-1 (quote (when #true (+ #1 #2)))))
```

Results are `#12` and `(if #true (do (+ #1 #2)))`. `eval` accepts a form, so parsing
text stays explicit. Declarations and control flow follow the same expression
scope rules as handwritten code; a standalone top-level `eval` can introduce
bindings visible to subsequent expressions. Errors point to the `eval` call.
`macroexpand-1` executes the macro body, including any side effects, but leaves
the resulting form unevaluated. Non-macro forms are returned unchanged.

`gensym` preserves identity when copied or spliced into syntax. `form/word` and
`form/variable` preserve that identity when converting between a binding name
and a reference. An ordinary text name cannot recreate it. Its prefix must be a
valid variable name. Generated identifiers display as `#<gensym:...>` or
`#<gensym-var:...>`; these display forms cannot be read back. `form/value` gives a
display name, not an interned identifier. Use the form itself to retain identity.

```parsh
(defmacro twice (expression)
  (let temporary (gensym temporary))
  (let reference (form/variable $temporary))
  `(do (let ,$temporary ,$expression) (+ ,$reference ,$reference)))
(dump (twice (+ #1 #2)))
```

This evaluates the expression once and returns `#6`. Generated bindings avoid
capturing caller variables. Other identifiers in templates still follow the
existing non-hygienic macro rules.

## Loading scripts

`(load path)` reads a UTF-8 source file, parses it completely, then evaluates its
top-level forms in the caller's surrounding expression scope. It returns the
last value, or nil for an empty file. Output uses the caller's streams; script
arguments are inherited. Each call executes the file again.

Top-level relative paths use the current working directory. While loading a
file, nested relative loads use that file's directory. Native files use canonical
paths for cycle checks. A load invoked later from a saved function follows the
current loading context or working directory. The limit is 32 active files and
8 MiB of source per file; cycles and invalid UTF-8 are errors.

```parsh
(load examples/lib/greetings.parsh)
(dump (greet-library parsh))
```

Native parsh reads its filesystem. Browser sessions read their virtual files;
custom `ExecutionHost` implementations can provide `read_file`, whose default
reports an unsupported loading capability. A syntax-only session has no files.
User module declarations are separate from executing a file with `load`.

Parse errors have kind `read`, file-access and load-limit errors have kind `load`,
and evaluation failures retain their kind. Messages include the loaded filename
and source position. Invalid syntax prevents the file from executing; a runtime
failure preserves effects from completed forms. `try` can catch failures, while
`return`, loop control, and `exit` keep their normal meaning.
