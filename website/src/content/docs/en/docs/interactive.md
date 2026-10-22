---
title: "Interactive editing"
description: "The parenthesis prompt, multiline editing, paste, and history."
---

- `(` supplies the outer opening parenthesis. Enter inserts a newline; a closing
  `)` at the end of the edit buffer submits a complete expression. Inner
  parentheses and parentheses inside strings or comments do not submit it.
  The implicit `(` is gray while input is blank and returns to the terminal's
  default text color when you start an expression. An explicitly entered `(`
  uses the default text color.
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
- History recall only restores source. At the end of an already complete
  history entry, press `)` to submit without adding another closing character.
- Ctrl+C clears the input or interrupts the foreground task. Ctrl+D exits on an
  empty buffer and deletes forward otherwise.

The browser playground shares this editor's input, history, paste, and layout model. It evaluates the language core; running Ctrl+C restarts the browser session. Native foreground programs and terminal handoffs apply to Linux only.
