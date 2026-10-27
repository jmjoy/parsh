---
title: "Interactive editing"
description: "Paredit structural editing, the parenthesis prompt, completion, paste, and history."
---

- The prompt is `$ ()` for regular users and `# ()` for root, with the cursor
  immediately before the closing parenthesis. The native terminal checks the
  effective user ID; the browser uses `$ ()`. Parentheses are gray while input
  is blank; the implicit `(` returns to the default color when you start typing.
  The prefix and gray closing hint are display-only.
- A first non-whitespace `#` switches the current input to direct literal source, hiding the implicit gray parentheses immediately. Deleting the prefix restores command input. Literal history entries keep their exact source and place the cursor at the end. Tab offers `#true`, `#false`, and `#nil`; paths starting with `#` are quoted when inserted.
- Enter checks the entire edit buffer, wherever the cursor is. Complete source
  executes immediately; otherwise parsh tries appending exactly one `)`. If the
  source is still unfinished, Enter inserts a newline at the cursor. Ctrl+J
  always inserts a newline. Blank input does not execute or enter history.
- Opening delimiters insert a matching closer. Starting fresh input with `(`, `[` or `{`
  switches to explicit source, so an expression keeps its own delimiters exactly once.
  Typing a closer moves past its match without executing; the implicit outer `)` stays a display hint.
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

### Paredit structural editing

Paredit is always enabled in the native terminal and browser playground. Typed `()`, `[]`,
`{}`, single quotes, and double quotes form pairs with point between them. Three consecutive
typed quotes promote a newly created empty quote pair to `'''…'''` or `"""…"""`.
Auto-inserted closers are real source: Enter can execute a complete expression even before
you type its closing delimiters. Use Ctrl+J to continue editing on another line.

Backspace/Delete remove both sides of an empty pair, and move across nonempty delimiter
boundaries. String escapes and triple-quote delimiters stay intact. Line and word kills
stop before a boundary they would split; Ctrl+T transposes ordinary text within a word or
string, preserving delimiters and escapes. Double-quoted text escapes a typed interior `"`;
inside a raw single-quoted string, typing `'` moves past its existing closer.

| Keys | Action |
| --- | --- |
| Ctrl+Alt+F/B | Move forward/backward over an expression |
| Ctrl+Alt+U/D | Move to the enclosing form's start / enter the next collection |
| Ctrl+Alt+K | Kill the next expression, using the shared kill ring |
| Alt+( | Wrap the current or next expression in `()`; insert an empty pair if none follows |
| Alt+S | Splice the innermost collection, removing its outer delimiters |
| Ctrl+Right/Left | Forward slurp/barf: move the closer to include/exclude one expression |
| Ctrl+Alt+Left/Right | Backward slurp/barf: move the opener to include/exclude one expression |

For example, forward slurp changes `(a) b` to `(a b)`; forward barf reverses it. Backward
slurp changes `a (b)` to `(a b)`. These transforms work with all three collection delimiters,
preserve source formatting, and add a separator when needed to keep words distinct. Each
transform is one undo group. Strings are whole expressions; embedded `$(...)` calls have
their own editing context. The implicit outer form and interpolation boundaries cannot
be spliced or reshaped.

Alt combinations also accept an Esc prefix: `Esc Ctrl+F` moves over an expression,
`Esc (` wraps, and `Esc Ctrl+Left` performs backward slurp. Paste and yank insert text
without automatic pairing, so incomplete or erroneous source can be pasted and repaired.
Unmatched characters can be deleted or closed; a structural transform with no reliable
target does nothing. History search queries use ordinary text editing.

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
| Ctrl+T | Transpose adjacent graphemes within ordinary text, or the last two at line end |
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

Errors include the source, line, and column. Script execution recovers
unexpected top-level `)` and continues with later expressions. Such errors give
a script exit status of 2 unless a runtime failure or explicit `exit` supersedes
it. Command failures keep their exit status; startup failures use 127 for a
missing command and 126 for permission denied. Other language errors use 2.

The initial scope is Linux foreground jobs, including interactive external
programs and foreground pipelines. There is no background job table, `bg`, `fg`,
or suspend/resume support; children ignore Ctrl+Z. History is session-local.
Macros and persistent configuration are future work.

The browser playground shares this editor's input, completion, history, paste, and layout model. It completes only commands and temporary files available in its isolated host. It evaluates the language and runs a coreutils subset; pressing Ctrl+C during execution resets the browser session. Native foreground programs and terminal handoffs apply to Linux only.
