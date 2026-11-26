# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/jmjoy/parsh/compare/v0.0.0...v0.1.0) - 2026-11-26

### Added

- [**breaking**] replace capture option maps with positional and keyword arguments
- [**breaking**] add keyword arguments and rename &optional to &opt
- [**breaking**] make capture operators explicit expression forms
- support lexical let rebinding with stable closure captures
- improve REPL guidance and source diagnostics
- add versioned AI agent skill guides to parsh
- add command aliases and persistent interactive history
- add native Parsh startup configuration
- add pipeline capture macros
- [**breaking**] rename capture and redirection builtins to arrow forms
- [**breaking**] unify capture options and default to trimmed text
- add functional helpers, metaprogramming, and source loading
- expand list and string standard library operations
- support chained struct field access with dot
- add generic dot builtin for dynamic struct field access
- [**breaking**] rename capture-call and add shorthand output helpers
- [**breaking**] make command capture and REPL output explicit
- expose the previous REPL value through $_
- add always-on Paredit editing to native and browser REPLs
- add dump builtin for debug output
- add JSON, native HTTP, and collection data helpers
- [**breaking**] namespace standard library collection operations
- [**breaking**] add execution results, exceptions, and native background tasks
- [**breaking**] enforce lexical scopes for parenthesized expressions
- [**breaking**] require typed numeric operands and integer indices
- [**breaking**] add typed scalar literals and explicit conversions
- [**breaking**] add first-class functions and standard library operations
- add structs and macros with quoted form support
- [**breaking**] make map commands operate on values instead of bindings
- [**breaking**] add bracket and brace shorthand for lists and maps
- add insertion-ordered maps and map operations
- add type builtin for command lookup inspection
- support triple-quoted multiline strings
- add while loops and break/continue control flow
- [**breaking**] add string interpolation and raw single-quoted strings
- Enter submits complete input with validated paste batches
- add numeric comparison operators
- add Emacs-style editing, kill ring, undo, and history search
- add arithmetic operators and numeric values
- commit prefix when typing `)` from middle of edit buffer
- add interactive Tab completion for commands, paths, and variables
- run uutils coreutils in the browser playground via WASI
- [**breaking**] bindings use bare names, references keep `$name`
- add website with documentation and WASM playground
- add |1>, |1>>, and |2>> redirection operators
- initial implementation of parsh, an S-expression shell

### Other

- Merge branch 'master' of https://github.com/jmjoy/parsh
- add Cargo installation instructions for parsh
- Make startup argument test independent of printf
- update examples for list and pipeline capture syntax
- offer MulanPSL-2.0, Apache-2.0, or MIT licensing
- expand bilingual guides and runnable examples
- use fixed-size array chunks for hex decoding
- replace lengthy READMEs with links to localized website docs
- highlight AI-friendly design across READMEs and website
- add runnable parsh examples and bilingual guides
- relocate website documentation to website/README.md
- Merge branch 'master' of https://github.com/jmjoy/parsh
- Make CLI argument tests utility-independent
- [**breaking**] move parsh into a workspace member crate
- replace manual CLI parsing with clap
