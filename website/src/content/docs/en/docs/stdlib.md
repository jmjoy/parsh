---
title: "Standard library"
description: "Immutable collections, JSON helpers, native HTTP requests, and numeric helpers."
---

These ordinary builtins support `(function name)` and `apply`. Data operations
share their implementation between native parsh and the browser; HTTP requests
and background tasks require native capabilities. Collection updates return new
values; use `set` to replace a binding. In signatures, brackets mark optional
arguments rather than literal list syntax, and `...` marks repeated arguments.

## Module namespaces

Text, list, and map operations use `module/member` names, such as
`str/join`, `list/append`, and `map/get`. These are ordinary builtin functions:

```parsh
(str/join [a b] ",")
(list/append [a] b)
(map/get {name parsh} name)
(apply (function str/join) [[a b] ","])
(apply "str/join" [[a b] ","])
```

The constructors are `str`, `list`, and `map`.
Names such as `str/join` are ordinary names. The slash is just a character in a
name or text value; it is a naming convention for grouping related functions,
not import syntax or a module object. A standalone `/` remains the division
builtin. Builtin namespaces are available directly, without imports. User module
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

## Type predicates

`typeof` returns a type name as text: `text`, `int`, `float`, `bool`, `list`,
`map`, `form`, `struct`, `bytes`, `function`, `command`, `nil`, or `status`.
Corresponding predicates are `text?`, `int?`, `float?`, `bool?`, `list?`, `map?`,
`form?`, `struct?`, `bytes?`, `function?`, `command?`, `nil?`, and `status?`.
`number?` accepts both integers and floats. `callable?` accepts function and command
reference values; it returns false for text names even though the call interface
can resolve them. `empty?` accepts only text, lists, and maps.

```parsh
(list (typeof (command echo)) (command? (command echo))
      (callable? (function +)) (callable? echo))
```

The result is `("command" #true #true #false)`.

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

## Data transforms

Higher-order list functions take the function first and return new values:

```parsh
(list/map (fn (value) (str $value "!")) [a b])
(list/filter (fn (value) (eq $value yes)) [yes no yes])
(list/fold (fn (total value) (+ $total $value)) #0 [#1 #2 #3])
```

| Call | Result |
| --- | --- |
| `(list/map f xs)` | Call `f` on each item and collect its return values |
| `(list/filter predicate xs)` | Keep items for which the predicate returns true |
| `(list/fold f initial xs)` | Call `f` with the accumulator and each item, from left to right |
| `(list/any predicate xs)` | Short-circuit at the first true result; false for an empty list |
| `(list/all predicate xs)` | Short-circuit at the first false result; true for an empty list |
| `(list/reverse xs)` | A new list in reverse order |
| `(list/contains xs value)` | Boolean membership using `eq` type-and-value equality |

Callbacks accept function values, external references, or text names supported by
`apply`. `filter`, `any`, and `all` require boolean callback results; command statuses
are not accepted as predicate results. `fold` returns `initial` for an empty list.

Maps support shallow merging and ordered entry conversion:

```parsh
(map/merge {name parsh} {version #1})
(map/entries {name parsh version #1})
(map/from-entries [[name parsh] [version #1]])
```

| Call | Result |
| --- | --- |
| `(map/merge maps...)` | New shallowly merged map; no arguments returns an empty map |
| `(map/entries m)` | Insertion-ordered list of two-item `[key value]` lists |
| `(map/from-entries entries)` | Map built from two-item lists with text keys |

Later maps overwrite earlier values while keeping the original key position;
new keys are appended. Nested values are replaced rather than recursively merged.
`map/from-entries` rejects malformed pairs and duplicate keys.

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

## JSON

`json/encode` returns compact JSON text. It accepts scalars, lists, maps, and
structs; struct fields become object properties. `json/decode` accepts UTF-8 text
or bytes and returns the corresponding parsh value: objects become maps, arrays
become lists, and `null` becomes nil.

```parsh
(defstruct User name admin)
(let body (json/encode (User parsh #true)))
(let value (json/decode $body))
(map/get $value name)
```

Encoding rejects functions, command references, forms, bytes, statuses, and
non-finite numbers. Decoding a struct's JSON returns a map; it does not reconstruct
the struct's name. JSON objects preserve input order; duplicate keys use the last
value. Decoded numbers use i64 when possible and finite f64 otherwise, so very
large JSON integers can lose precision. JSON failures have `Error.kind` equal to `json`.

## Bytes

`bytes/` functions operate on raw bytes returned by command capture and HTTP:

| Call | Result |
| --- | --- |
| `(bytes/len value)` | Byte length. |
| `(bytes/slice value start [end])` | Byte slice using list index rules. |
| `(bytes/concat values...)` | Concatenated bytes. |
| `(bytes/from-text text)` / `(bytes/to-text bytes)` | UTF-8 conversion; decoding is strict. |
| `(bytes/base64-encode bytes)` / `(bytes/base64-decode text)` | Standard Base64 conversion. |
| `(bytes/hex-encode bytes)` / `(bytes/hex-decode text)` | Lowercase hex encoding and strict decoding. |

Indices and bounds must be ints. Negative bounds count from the end, out-of-range
bounds are clamped, and a reversed range returns empty bytes. Byte slicing can
split a UTF-8 sequence; decoding then fails. Hex decoding accepts uppercase or
lowercase digits, requires an even length, and rejects whitespace and non-hex text.

```parsh
(let raw (bytes/hex-decode "00ff41"))
(list (bytes/len $raw) (bytes/base64-encode $raw)
      (bytes/to-text (bytes/slice $raw #-1)))
```

The result is `(#3 "AP9B" "A")`. Bytes display as `<bytes N>` and cannot implicitly
become text; use an explicit decoder or encoding.

## Debug output

`dump` prints any parsh value using its display representation and adds a
trailing newline. It returns nil, and follows the same stdout redirection and
capture rules as other builtin output:

```parsh
(dump {name parsh items [#1 #true]})
(let result (capture-call printf "hello\n"))
(dump (CallResult.stdout $result))
```

Use `capture-call` when the printed representation is needed as text:

```parsh
(stdout-text (capture-call dump {name parsh}))
```

The result is `"(map \"name\" \"parsh\")\n"`. Ordinary `(let value (dump ...))`
binds nil because `dump` returns nil even when it writes output.

## HTTP

Native parsh provides synchronous requests through `http/request`, `http/get`,
and `http/post`. These request functions report an unsupported capability in the
browser; `http/body-text` only decodes an existing response and shares the data
implementation.

This example assumes an HTTP service returning JSON at the given URL. Replace it
with your service's endpoint:

```parsh
(let response
  (http/get "http://127.0.0.1:8080/data"
            {accept [application/json]}))
(if (HttpResponse.ok $response)
    (json/decode (HttpResponse.body $response))
    (HttpResponse.status $response))
```

The available calls are:

| Call | Result |
| --- | --- |
| `(http/request method url [body] [headers])` | An `HttpResponse`; use `#nil` when a custom method has headers but no body. |
| `(http/get url [headers])` | A GET response. |
| `(http/post url body [headers])` | A POST response. |
| `(http/body-text response)` | Strict UTF-8 text from the response body. |

Request bodies accept nil, text, or bytes. Headers are maps from text names to
lists of text values. Response headers use lowercase names and preserve repeated
values in their lists. `HttpResponse` contains `status`, `ok`, `headers`, `body`
as bytes, and the final `url`. A 4xx or 5xx response is still returned as data;
DNS, connection, protocol, and body-limit failures throw an error with kind `http`.
`ok` is true only for 2xx statuses. Each request has a 30-second overall timeout and
an 8 MiB response-body limit. `HttpResponse` is a runtime-owned struct: inspect its
fields with accessors such as `HttpResponse.status`; its constructor cannot be
called or redefined.

## Calls, results, and tasks

`command`, `command?`, `callable?`, `bytes?`, `capture-call`, `stdout-text`, `stderr-text`, `exit-code`, `signal`, `check`, and `throw` work in native and browser sessions. `spawn`, `wait`, `job-status`, and `cancel` require native task support. All are ordinary builtins available through `function` and `apply`. See [execution results and tasks](/parsh/en/docs/execution/) for signatures and result structures.
