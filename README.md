# parsh

English | [简体中文](README.zh-CN.md)

parsh stands for **parenthesis shell**.

A Linux shell whose command line and scripts use the same Lisp dialect. The
prompt is `$ ()` for regular users and `# ()` for root. Type `echo hello`
and press Enter to run `(echo hello)`.
Ctrl+J inserts a newline.

This is an early implementation, version `0.1.0`.

## Run

```console
cargo run
cargo run -- -c '(echo hello)'
cargo run -- examples/hello.parsh world
```

With no arguments, parsh opens an interactive editor when stdin is a terminal,
otherwise it reads a script from stdin. `parsh -` explicitly reads stdin.
`--help` and `--version` are also available. Script arguments are the list
`$args`; it excludes the script filename. Scripts can start with
`#!/usr/bin/env parsh` once the binary is on PATH.

Options are parsed by clap. After the script path or `-c SOURCE`, every remaining
argument is passed through unchanged, including `--help` and `--`.
Use `parsh -- -script.parsh` for a script filename beginning with `-`.

See [examples](examples/README.md) for runnable scripts covering variables,
functions, loops, pipelines, and file operations.

## Variables and values

In value positions, bare words are text: `123`, `true`, `false`, and `nil` have no
special literal meaning. Double-quoted strings interpolate `$name` and `$(...)`,
and support `\"`, `\\`, `\n`, `\r`, `\t`, and `\$`. Single-quoted strings preserve
their content without interpolation or escapes. `${name}` remains literal text.
Triple-quoted strings, `"""..."""` and `'''...'''`, preserve newlines and indentation.
Triple double quotes support the same interpolation and escapes; triple single quotes preserve everything literally.
Outside strings, variable references use `$name`. Binding
and assignment targets use bare names, including function parameters, loop
bindings, and environment settings. Names start with a letter or `_`, followed
by letters, digits, or `_`. Binding targets must be bare names, not quoted strings
or expressions.

```lisp
(let name "a friend")
(echo hello $name)
(set name world)
(echo "hello, $name")
(echo "sum=$(+ 1 2)")
(echo 'literal $name and $(pwd)\n')
(echo """hello, "$name"
sum=$(+ 1 2)""")
(echo '''literal '$name'
$(pwd)\n''')
(export EDITOR vim)
```

`let` binds in the current lexical scope. `set` updates the nearest existing
binding, or an existing environment entry, and errors if the name is undefined.
Lookups fall back to the inherited environment. `export` sets the environment for
subsequent children and updates an existing local binding of the same name.
Environment and working directory changes belong to this parsh session.

`(get name)` is the function form of `$name`: it takes one text variable name
and returns the original value, using the same lookup rules. Use
`"$(get name)_suffix"` to delimit a variable name. `$(name)` calls the zero-argument
function or command `name`; interpolation accepts all existing expressions,
including nested calls and captured command output. Parts evaluate once from
left to right and convert scalars as `str` does; nil becomes empty text, while
lists and maps are rejected. The result stays one argument, and inserted values are not parsed
again. Use `\$` or single quotes when passing a literal `$` to another program.
Single quotes now delimit strings; use double quotes for words with apostrophes.

```lisp
(let files (list "a file.rs" other.rs))
(printf "<%s>\n" $files)
(for file (glob *.rs)
  (echo $file))
```

Lists expand by one level into external command arguments. Text stays a single
argument, including spaces and embedded newlines. `list` constructs lists;
`str` concatenates scalar values. `glob` is explicit, returns sorted paths, and
returns an empty list when nothing matches. Ordinary words, strings, and variable
values never undergo implicit globbing, word splitting, or tilde expansion.

`[a b]` is shorthand for `(list a b)`, and `[]` creates an empty list.
`{name parsh}` is shorthand for `(map name parsh)`, and `{}` creates an empty map.
The forms can nest: `[one {items [two three]}]`. Elements are separated by whitespace;
maps use alternating keys and values. Quote text and glob patterns containing
`[]` or `{}`, for example `(glob 'file[12].txt')`.

`cd` accepts a path, or defaults to `$HOME`; it updates `$PWD` and `$OLDPWD`.
Relative commands, glob patterns, and redirections use the current directory.

## Maps

`map` takes alternating text keys and values. `(map)` creates an empty map.
Keys and values evaluate once from left to right; duplicate keys are errors,
reported before evaluating the duplicate key's value. Values may contain lists
and nested maps.

```lisp
(let config (map name parsh version (+ 1)))
(mset config name new-name)
(mget config name)
(mget config missing fallback)
(mkeys config)
(mdel config version)
```

All `m` commands take a bare variable name as their first argument, as in
`(mget config name)`. They access the nearest lexical binding, which must hold a map.

| Command | Result |
| --- | --- |
| `(mget config key)` | Stored value; a missing key is an error |
| `(mget config key default)` | Evaluate `default` only if the key is missing |
| `(mset config key value)` | Insert or replace the value; return nil |
| `(mdel config key)` | Delete the key; return nil; a missing key is an error |
| `(mhas config key)` | Boolean indicating whether the key exists |
| `(mkeys config)` | List of keys in insertion order |
| `(mvals config)` | List of values in the same order |
| `(mlen config)` | Integer entry count |

Replacing a value keeps its position; new or reinserted keys go at the end.
Maps display as `(map "name" "parsh")`. `eq` compares their contents regardless
of insertion order. Assignment and function arguments copy maps, including
nested values: changing one variable does not change its copies. Maps cannot
be converted implicitly to text, command arguments, or environment values.

## Arithmetic and comparisons

Bare words remain text. Arithmetic operators `+`, `-`, `*`, `/` and comparison
operators `=`, `!=`, `<`, `<=`, `>`, `>=` accept numbers or numeric text,
including variables and captured command output. Arithmetic returns a
number; ordinary text such as `00123` keeps its original spelling.

```lisp
(let count (+ 001 2))
(+ $count 1)
(/ 5 2)
(eq $count 3)
(= $count 3.0)
(str 00123)
```

After the binding, an interactive session displays `4`, `2.5`, `false`, `true`,
and `"00123"`. `eq` compares types and values, so text, integers, and floats are
distinct. `=` compares numeric values, including numeric text; it takes exactly
two arguments and does not use a floating-point tolerance.

`!=` tests numeric inequality; `<`, `<=`, `>`, and `>=` test numeric order.
Each comparison takes exactly two arguments and returns a boolean. Comparisons
preserve integer precision even when comparing a large integer with a float.

```lisp
(> 10 2)
(>= 3 3.0)
(< 9007199254740992.0 9007199254740993)
(!= 1 1.5)
(if (<= $count 3) (echo "at most three"))
```

Arguments are evaluated once, from left to right; arithmetic operations fold left.
`+` and `*` accept any number of arguments, with `(+)` returning integer `0` and
`(*)` returning integer `1`. `-` and `/` require at least one argument; one
argument means negation or reciprocal. Division always returns a float.

Numeric text is trimmed and parsed completely as signed decimal integers,
decimal fractions, or scientific notation. Leading zeroes are decimal; integer
text must fit in `i64`. Booleans, nil, lists, and maps are not numeric operands.
Integer addition, subtraction, and multiplication are exact and check for
overflow. Each operation involving a float uses `f64`, which can lose integer
precision and has ordinary floating-point rounding: `(+ 0.1 0.2)` returns
`0.30000000000000004`. Invalid operands, division by zero, integer overflow,
and non-finite floats are language errors with exit status `2`.
Numbers display without quotes; floats retain a decimal point or exponent,
such as `4.0`. `str`, command arguments, and environment values use that same
format. Files and `-c` scripts require an explicit command such as `echo` to
print results.

## Structured comments

```lisp
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

## Commands and composition

Command lookup checks built-ins, named functions, then PATH. External programs
are started directly, without passing the expression through another shell.
The command name may also come from a text variable, as in `($EDITOR file.txt)`;
its value is one program name, without implicit splitting into flags.

Use `type` to inspect command lookup without executing the command:

```lisp
(type type cd echo)
(defn greet (name) (echo hello $name))
(type greet)
(type -a echo)
(let kind (type -t greet))
(if (type -t optional-tool) (optional-tool))
```

`type` accepts multiple names, including list arguments, and supports these Bash-style options:

- `-a`: show every match, including all executable paths in PATH order.
- `-t`: print `builtin`, `function`, or `file` for each match.
- `-p`: print the external command path, or nothing for a built-in or function.
- `-P`: search PATH even when a built-in or function has the same name.
- `-f`: skip function lookup.

Options can be combined; `--` ends option parsing. No arguments succeeds silently.
An unknown name gives status `1`; an invalid option gives status `2`.
Output supports capture and redirection. In the browser playground, external
commands are reported by the host's available command names; `-p` and `-P`
return those names.

```lisp
(echo (pwd))
(| (find . -name "*.rs") (sort))
(|> files.txt (| (find . -name "*.rs") (sort)))
(|>> log.txt (echo another entry))
(<| input.txt (sort))
(|2> errors.txt (grep needle missing.txt))
```

The operators are:

- `|`: connect external commands in a pipeline.
- `|>`, `|1>`: create or truncate a file and redirect stdout.
- `|>>`, `|1>>`: create or append a file and redirect stdout.
- `<|`: read stdin from a file.
- `|2>`: create or truncate a file and redirect stderr.
- `|2>>`: create or append a file and redirect stderr.

Redirection always takes the path first and an expression second. Wrappers can
nest and can enclose an entire pipeline. Pipeline stages in v1 must be external
command calls; functions and state-changing built-ins run outside pipelines.

For example, write stdout to a fresh log and append stderr to another:

```lisp
(|1> output.log
  (|2>> errors.log
    (make)))
```

Top-level commands stream output immediately. When an external command or
pipeline is used as a value, stdout is captured as UTF-8, trailing LF characters
are removed, and the result stays one argument. Stderr keeps its existing
destination. Use a pipeline or file redirection for binary output. Explicit
redirection takes precedence over capturing that expression's output.

## Control flow and functions

```lisp
(defn greet (name)
  (echo hello $name))

(if (test -d src)
    (greet developer)
    (echo missing))

(if (eq $EDITOR vim)
    (echo using vim))

(if (not (test -f config.txt))
    (echo configuration missing))

(do
  (let message "multiple expressions")
  (echo $message))
```

`if` evaluates only the selected branch. A condition must produce a boolean
(from `eq`, a numeric comparison, or `not`) or a command status; zero status is true. Text and
numbers are not implicitly booleans. `do` runs expressions in sequence in the current scope.
`for` evaluates a list and creates a fresh lexical binding for each iteration.

`while` checks its condition before each iteration. Both loop forms accept one or
more body expressions and create a fresh scope per iteration: `let` stays local,
and `set` can update an outer binding.

```lisp
(let count 0)
(while (< $count 5)
  (set count (+ $count 1))
  (if (= $count 2) (continue))
  (if (= $count 4) (break (list stopped $count)))
  (echo iteration $count))
```

`(continue)` skips the rest of the current iteration. `(break)` ends the nearest
loop; `(break value)` evaluates `value` once and returns it from the loop,
including numbers, lists, maps, booleans, or `nil`. Without an explicit break value,
a loop returns its last complete iteration's result, or `nil` if none completed.
Skipped or interrupted iterations do not replace that result. The example prints
`iteration 1` and `iteration 3`, and returns `("stopped" 4)`.

Loop jumps cannot cross a function boundary. Using one without an active loop
body in the current function is an error. A loop's condition or list expression
runs outside its own body, so jumps there target an existing outer loop. A command
failure in a `while` body stops execution, including when the loop is used as a condition.

Function names and parameter declarations use bare names. Read parameter and
local variable values with `$name`. Functions capture their defining lexical
scope and return the last expression's value. Argument expressions are evaluated
before parameters are bound. Functions can return numbers, lists, maps, and booleans as well as
command output.
When called as a value, stdout is captured throughout the function body: if the
last expression produces a command status, captured stdout is returned as text;
otherwise the last typed value is returned. Earlier captured output is not
printed in that case.

An ordinary nonzero command status stops the current script or submitted batch.
Conditions may inspect nonzero statuses, but missing programs and other startup
errors still fail. A pipeline reports its rightmost nonzero stage status, even
if the last stage succeeds (including upstream SIGPIPE). Every stage is reaped.
`exit` accepts a code from 0 to 255 and exits immediately, including from a
function or nested expression. Its default is 0.

## Interactive editing and paste

- The prompt is `$ ()` for regular users and `# ()` for root, with the cursor
  immediately before the closing parenthesis. The native terminal checks the
  effective user ID; the browser uses `$ ()`. Parentheses are gray while input
  is blank; the implicit `(` returns to the default color when you start typing.
  The prefix and gray closing hint are display-only.
- Enter checks the entire edit buffer, wherever the cursor is. Complete source
  executes immediately; otherwise parsh tries appending exactly one `)`. If the
  source is still unfinished, Enter inserts a newline at the cursor. Ctrl+J
  always inserts a newline. Blank input does not execute or enter history.
- Typing `)` only edits text. Starting fresh input with `(` switches to explicit
  source, so a complete expression or script keeps its own parentheses exactly once.
- Pasting only inserts text, including newlines and tabs; it never executes.
  Press Enter to submit. Every top-level expression must be complete and valid
  before any expression in the batch executes. Syntax errors report their
  position and preserve the draft and undo history for correction. An unfinished
  batch stays editable without executing its complete prefix.
- A runtime failure stops later expressions in the submitted batch; earlier
  effects remain. Ctrl+C cancels the current task and the rest of the batch.
  Queued source stays in parsh; an interactive child reads subsequent terminal
  input, not queued commands.
- Arrow keys, Home/End, Backspace/Delete, Ctrl+A/E, and Ctrl+U/K edit text.
  Editing and display account for Unicode grapheme clusters and character
  width. Up/Down move between lines and recall session history at the buffer
  boundaries; Ctrl+P/N always browse history.
- Tab completes built-ins, defined functions, executable commands, file paths,
  and `$variable` names. A unique match is inserted directly; directories end in
  `/`. Multiple matches extend the common prefix; press Tab again to list them.
  Completion works at the cursor, handles quoted paths, and never runs a command.
  `cd` completes directories only; hidden files require a `.` prefix. Strings do
  not expand variables, and structural comments are not completed. Pasted tabs
  remain literal tabs.
- History keeps each executed top-level expression in full. Recall displays its
  complete parentheses and places the cursor immediately before the outer `)`.
  Append arguments directly, or press Enter
  to run it again. Returning to your draft restores it unchanged.
- Ctrl+C clears the input or interrupts the foreground task. Ctrl+D exits on an
  empty buffer and deletes forward otherwise.

### Emacs-style shortcuts

The native terminal and browser playground share these editing commands. Alt combinations
also work by pressing Esc followed by the corresponding key. Character editing uses Unicode
grapheme clusters; Home/End, Ctrl+A/E, and Ctrl+U/K operate on the current logical line.

| Keys | Action |
| --- | --- |
| Ctrl+J | Insert a newline without executing |
| Ctrl+B/F, Ctrl+H | Move left/right; delete backward |
| Alt+B/F | Move to the previous word's start/next word's end |
| Ctrl+W | Kill backward, using whitespace to separate words |
| Alt+Backspace, Alt+D | Kill backward/forward; words contain letters and digits, with path punctuation and underscores as separators |
| Ctrl+U/K | Kill to the line's start/end; Ctrl+K at the end kills the newline |
| Ctrl+Y, Alt+Y | Yank killed text; immediately cycle through earlier kills |
| Ctrl+T | Transpose adjacent graphemes, or the last two at line end; never cross a newline |
| Ctrl+L | Clear and redraw the screen, preserving the draft and session |
| Ctrl+R/S | Search session history incrementally backward/forward |
| Ctrl+_, Ctrl+X Ctrl+U | Undo an editing group |

Consecutive kills combine in the direction of deletion; movement or other editing commands
end the sequence. The kill ring survives command submission and is cleared on session reset.
Alt+Y works only immediately after Ctrl+Y or Alt+Y. Yanking, transposing, and undoing only
edit text: a `)` restored by those commands never executes a command.

History search matches case-sensitive substrings of complete source without wrapping.
Typing or pasting edits the query; Backspace removes a grapheme. Repeat Ctrl+R/S for the
previous/next match; repeating with an empty query reuses the last search. A failed search
keeps the last preview and shows a failure prompt. Enter or Esc accepts the result for
editing without executing or inserting a newline; press Enter again afterward to execute.
Ctrl+G restores the original draft and cursor. Other editing commands accept the result
before acting; Ctrl+L preserves the search and Ctrl+C cancels the input.

Continuous typing is one undo group. Deletions, kills, yanks, transpositions, completions,
and unsubmitted pastes each form separate groups. Movement ends a typing group. History
navigation, search acceptance, and submission establish new undo baselines; interruption
and clearing input also clear undo history. Undo never reverses executed commands or their effects.

Browser shortcuts apply while the terminal has focus. Some browser or operating-system
shortcuts cannot reach the page; use an Esc prefix when Alt conflicts. Copy and paste
continue using the browser's shortcuts.

Errors include a source, line, and column. Script execution recovers
unexpected top-level `)` and continues with later expressions. Such errors give
a script exit status of 2 unless a runtime failure or explicit `exit` supersedes
it. Command failures keep their exit status; startup failures use 127 for a
missing command and 126 for permission denied. Other language errors use 2.

The initial scope is Linux foreground jobs, including interactive external
programs and foreground pipelines. There is no background job table, `bg`, `fg`,
or suspend/resume support; children ignore Ctrl+Z. History is session-local.
Macros and persistent configuration are future work.

## Development

```console
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Tests include real subprocesses, large pipelines, file redirection, lexical
functions, and Linux PTYs for paste, history, Unicode editing, foreground
interrupts, terminal restoration, and separation of queued source from child
stdin.

For website development and deployment, see [website/README.md](website/README.md).

## License

Licensed under the Mulan Permissive Software License, Version 2.
See [LICENSE](LICENSE) for details.
