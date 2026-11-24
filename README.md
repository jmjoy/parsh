# parsh

English | [简体中文](README.zh-CN.md)

**Your shell, in parentheses.**

parsh (**parenthesis shell**) is a Linux shell whose interactive commands and
scripts share the same small Lisp dialect. Familiar commands, with a little more
structure.

Built in Rust around S-expressions, explicit values, and composable commands.
The project is open source and in early development.

[Website](https://jmjoy.github.io/parsh/en/) ·
[Playground](https://jmjoy.github.io/parsh/en/playground/) ·
[Documentation](https://jmjoy.github.io/parsh/en/docs/)

## One language for the prompt and scripts

Take an expression from the prompt straight into a script. Variables, functions,
and control flow follow the same rules in both. Command calls are expressions:

```parsh
(let name world)
(dump (str "hello, " $name))

(defn greet (who)
  (str "hello, " $who))
(dump (greet parsh))
```

Both interactive sessions and scripts print explicitly with `echo`, `printf`, or
`dump`. The two `dump` calls above display `"hello, world"` and `"hello, parsh"`.
Expression return values are never printed automatically.

## Arguments stay intact

Text stays one argument. Lists expand one level into external command arguments.
No implicit word splitting or globbing, and no guessing what spaces will do:

```parsh
(let files ["a file.rs" "b.rs"])
(dump [$files])
```

## Real commands, structured composition

On Linux, programs launch directly. Compose commands with pipes and redirections.
Keep your familiar tools and use a new syntax:

```parsh
(out-> files.txt
  (| (find . -name "*.rs") (sort)))
```

## Logic and commands, one grammar

For people and AI agents alike, arithmetic, data, and control flow stay in the
script; programs, pipes, and redirections are expressions too. With fewer quoting
layers to guess, generated scripts read the way they run:

```parsh
(let count (int (<-|out (ls) (wc -l))))
(echo "entries: $count; doubled: $(* $count #2)")
```

Try parsh and coreutils in the [browser playground](https://jmjoy.github.io/parsh/en/playground/)
without installing anything. To work with your own files and programs on Linux,
see [Getting started](https://jmjoy.github.io/parsh/en/docs/getting-started/).
The [documentation](https://jmjoy.github.io/parsh/en/docs/) covers the language
reference and development guides; [examples](examples/README.md) provide complete
scripts organized by topic.

## License

Licensed under your choice of:

- [Mulan Permissive Software License, Version 2](LICENSE-MULAN) (`MulanPSL-2.0`)
- [Apache License, Version 2.0](LICENSE-APACHE) (`Apache-2.0`)
- [MIT License](LICENSE-MIT) (`MIT`)

SPDX expression: `MulanPSL-2.0 OR Apache-2.0 OR MIT`.

Third-party code retains its original license terms. The vendored [uutils pwd](crates/parsh-coreutils/vendor/pwd/) is licensed under MIT.
