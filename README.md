# parsh

English | [简体中文](README.zh-CN.md)

parsh stands for **parenthesis shell**.

A Linux shell whose command line and scripts use the same Lisp dialect. The
prompt is `(`. Type `echo hello)` to run `(echo hello)`; Enter inserts a newline.

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

## Variables and values

In value positions, bare words are text: `123`, `true`, `false`, and `nil` have no
special literal meaning. Double-quoted strings support `\"`, `\\`, `\n`, `\r`, and `\t`.
Strings do not interpolate variables. Variable references use `$name`. Binding
and assignment targets use bare names, including function parameters, loop
bindings, and environment settings. Names start with a letter or `_`, followed
by letters, digits, or `_`. Binding targets must be bare names, not quoted strings
or expressions.

```lisp
(let name "a friend")
(echo hello $name)
(set name world)
(echo (str "hello, " $name))
(export EDITOR vim)
```

`let` binds in the current lexical scope. `set` updates the nearest existing
binding, or an existing environment entry, and errors if the name is undefined.
Lookups fall back to the inherited environment. `export` sets the environment for
subsequent children and updates an existing local binding of the same name.
Environment and working directory changes belong to this parsh session.

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

`cd` accepts a path, or defaults to `$HOME`; it updates `$PWD` and `$OLDPWD`.
Relative commands, glob patterns, and redirections use the current directory.

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
text must fit in `i64`. Booleans, nil, and lists are not numeric operands.
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

Function names and parameter declarations use bare names. Read parameter and
local variable values with `$name`. Functions capture their defining lexical
scope and return the last expression's value. Argument expressions are evaluated
before parameters are bound. Functions can return numbers, lists, and booleans as well as
command output.
When called as a value, stdout is captured throughout the function body: if the
last expression produces a command status, captured stdout is returned as text;
otherwise the last typed value is returned. Earlier captured output is not
printed in that case.

An ordinary nonzero command status stops the current script or paste batch.
Conditions may inspect nonzero statuses, but missing programs and other startup
errors still fail. A pipeline reports its rightmost nonzero stage status, even
if the last stage succeeds (including upstream SIGPIPE). Every stage is reaped.
`exit` accepts a code from 0 to 255 and exits immediately, including from a
function or nested expression. Its default is 0.

## Interactive editing and paste

- `(` supplies the outer opening parenthesis. Enter inserts a newline. Typing
  `)` submits the source to the left of the cursor, including the new `)`, when
  it forms a complete expression. Inner parentheses and parentheses inside
  strings or comments do not submit it.
  The implicit `(` is gray while input is blank and returns to the terminal's
  default text color when you start an expression. An explicitly entered `(`
  uses the default text color.
- Submitting from the middle of the edit buffer preserves the text to the right
  as the next draft, with the cursor at its start. The draft is not executed
  automatically and is preserved even if evaluation of the submitted source
  fails. This applies to typed `)` characters; paste follows the batch rules below.
- Starting a fresh input with `(` switches to explicit source mode: the
  prompt's implicit opening parenthesis disappears. Pasting a complete script
  therefore shows and reads its own parentheses exactly once.
- Bracketed paste is processed in order. Each complete expression executes
  before the remainder is consumed. After the first expression, the remainder
  of that paste is explicit source, with no new implicit opening parenthesis.
  An unfinished tail remains editable.
- Every unexpected top-level `)` produces its own position-tagged error and is
  consumed. Later valid expressions still execute. For example,
  `(echo a))) (echo b)` prints a, reports two errors, then prints b.
- A runtime failure stops further evaluation of that batch, but the reader
  continues reporting extra closing parentheses in the received source. Ctrl+C
  cancels the current task and the rest of its paste batch.
- Remaining pasted source stays in parsh's queue; an interactive child reads
  subsequent terminal input, not that queued source.
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
- History keeps complete source. Recalling an entry removes its outer closing
  `)` from the edit buffer and places the cursor at the end. Append arguments
  directly, or press `)` to run it again. Returning to your draft restores it
  unchanged.
- Ctrl+C clears the input or interrupts the foreground task. Ctrl+D exits on an
  empty buffer and deletes forward otherwise.

### Emacs-style shortcuts

The native terminal and browser playground share these editing commands. Alt combinations
also work by pressing Esc followed by the corresponding key. Character editing uses Unicode
grapheme clusters; Home/End, Ctrl+A/E, and Ctrl+U/K operate on the current logical line.

| Keys | Action |
| --- | --- |
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
editing without executing or inserting a newline; press `)` afterward to execute.
Ctrl+G restores the original draft and cursor. Other editing commands accept the result
before acting; Ctrl+L preserves the search and Ctrl+C cancels the input.

Continuous typing is one undo group. Deletions, kills, yanks, transpositions, completions,
and unsubmitted pastes each form separate groups. Movement ends a typing group. History
navigation, search acceptance, and submission establish new undo baselines; interruption
and clearing input also clear undo history. Undo never reverses executed commands or their effects.

Browser shortcuts apply while the terminal has focus. Some browser or operating-system
shortcuts cannot reach the page; use an Esc prefix when Alt conflicts. Copy and paste
continue using the browser's shortcuts.

Errors include a source, line, and column. Script execution also recovers
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
