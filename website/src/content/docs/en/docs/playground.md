---
title: "The playground"
description: "Run parsh, coreutils, and command pipelines in your browser."
---

[Open the interactive terminal](/parsh/en/playground/) to run the Rust parsh interpreter and real uutils coreutils compiled to WebAssembly.

## Your first command

The prompt supplies an opening parenthesis. Type `echo hello)` to execute it, or paste complete expressions:

```parsh
(echo "hello, parsh")
(let here (pwd))
(echo (str "working in " $here))
(| (sort fruits.txt) (uniq -c))
(|> results.txt (<| numbers.txt (sort -n)))
(cat results.txt)
```

Commands print their output directly. Expressions such as `(str hello)` display their return value (for example, the text `"hello"`). Nil and command exit statuses are not printed.

## Commands and files

The following 29 commands use the real uutils implementations:

`echo`, `printf`, `pwd`, `ls`, `cat`, `head`, `tail`, `sort`, `uniq`, `wc`, `seq`, `cut`, `tr`, `tee`, `nl`, `base64`, `sha256sum`, `basename`, `dirname`, `mkdir`, `rmdir`, `touch`, `cp`, `mv`, `rm`, `test`, `true`, `false`, `printenv`.

The initial working directory is `/home/playground`, containing `fruits.txt`, `numbers.txt`, and `README.txt`. `/tmp` is also available. `HOME`, `PWD`, `LANG`, and `TERM` are initialized for this isolated session; `$args` is empty. `export` changes the environment passed to subsequent commands.

Files, `cd`, explicit `glob`, command capture, pipes, and all parsh redirection operators work within the same temporary filesystem:

```parsh
(mkdir -p demo)
(cd demo)
(|> greeting.txt (echo hello))
(|>> greeting.txt (echo parsh))
(cp greeting.txt copy.txt)
(printf "%s\n" (glob *.txt))
(if (test -f copy.txt) (cat copy.txt) (echo missing))
(cd ..)
```

Nested commands capture UTF-8 stdout, remove trailing LF characters, and keep the result as one argument. Explicit output redirections inside the captured expression take precedence. Files and pipelines preserve binary bytes.

Browser pipelines run one stage at a time with buffered output. They report the rightmost nonzero status, but do not reproduce native streaming or SIGPIPE behavior. stdin is EOF unless supplied by a file or pipeline. Each command's output and each pipeline buffer are limited to 8 MiB; the virtual filesystem is limited to 32 MiB. Commands such as `grep`, `sed`, and `find` and interactive programs are not included.

## Sessions and keys

- Enter inserts a newline. A closing outer `)` evaluates the expression.
- Up/Down move between lines or recall history. Ctrl+P/N always browse history. Press `)` at the end of a complete history entry to run it again.
- Unicode editing, multiline input, full-script paste, and batch error recovery use the shared native editor model.
- Clear and idle Ctrl+C preserve files, bindings, and the working directory.
- Reset, refresh, or leaving the page ends the temporary session; browser back-forward caching can temporarily preserve a page.
- During execution, Ctrl+C immediately stops and resets the entire session.
- An expression running for more than 5 seconds automatically resets the session, including files, variables, functions, environment, and history.
- `exit` and Ctrl+D on empty input end the session; reset to restart.

To work with your own files and Linux programs, [run parsh on Linux](/parsh/en/docs/getting-started/).
