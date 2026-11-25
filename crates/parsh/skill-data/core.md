# Using parsh

parsh evaluates S-expressions. A call has the form `(name arg ...)`, and
every nested call uses the same syntax. Bare words are text, `$name` reads
a binding, and `#2` and `#true` are typed literals. Use `dump` to
display an expression's value.

Pass a short expression to `parsh -c`. Single quotes in the calling shell
keep parsh's variable references intact.

```console
parsh -c '(let count #2) (dump (+ $count #3))'
```

This prints `#5`. Use `parsh FILE` for a script or `parsh -` to read a
script from stdin. Inspect both the output and the exit status when checking
generated code.

## Find a topic

Use `parsh skills list` to see all available topics or `parsh skills get`
with one or more names to see just those guides.

```console
parsh skills get syntax variables
parsh skills get commands --full
```

Start with `syntax` for quoting, literals, and interpolation. Read `commands`
for argument passing, pipelines, and redirection, or `execution` for captured
results and error handling. Use `stdlib` to find built-in data operations and
`control-flow` for functions, conditions, and loops.

Guide links such as `/parsh/en/docs/execution/` use the corresponding topic
name. Load that topic locally to keep the reference matched to this binary.
Add `--full` to include related example source, or `--json` for structured
output.
