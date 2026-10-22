use std::{
    fs,
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

fn quote(text: &str) -> String {
    format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}

fn invoke(args: &[&str], stdin: &str) -> Output {
    invoke_in(args, stdin, None)
}

fn invoke_in(args: &[&str], stdin: &str, directory: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_parsh"));
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let mut child = command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(stdin.as_bytes()).unwrap();
    drop(input);
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(8);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(child.id() as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
            let _ = child.wait();
            panic!("parsh timed out: {args:?}");
        }
        thread::sleep(Duration::from_millis(5));
    };
    Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    }
}

fn run(source: &str) -> Output {
    invoke(&["-c", source], "")
}
fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}
fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}
fn success(output: &Output) {
    assert!(output.status.success(), "{}", stderr(output));
}

#[test]
fn literal_atoms_variables_and_parameter_boundaries() {
    let output = run(r#"(let name "a b") (printf "<%s>\n" $name 123 true nil ; a;b)"#);
    success(&output);
    assert_eq!(stdout(&output), "<a b>\n<123>\n<true>\n<nil>\n<;>\n<a;b>\n");
    for source in [
        "(let $name x)",
        "(set $name x)",
        "(for $item (list a) (echo bad))",
        "(defn f ($name) (echo bad))",
        "(export $NAME x)",
    ] {
        let output = run(source);
        assert_eq!(output.status.code(), Some(2), "{source}");
        assert!(
            stderr(&output).contains("binding must use a bare variable name without '$'"),
            "{}",
            stderr(&output)
        );
    }
}

#[test]
fn comments_are_reader_forms_not_evaluated_arguments() {
    let output =
        run(r#"(; $undefined (absent-command) (; nested ")")) (printf "<%s>" (; ignored) yes)"#);
    success(&output);
    assert_eq!(stdout(&output), "<yes>");
    let output = run(r#"(";" x)"#);
    assert_eq!(output.status.code(), Some(127));
}

#[test]
fn nested_commands_capture_complete_stdout_without_splitting() {
    let source = format!(
        "(printf \"<%s>\" (sh -c {}))",
        quote("printf warning >&2; printf 'a b\nc\n\n'")
    );
    let output = run(&source);
    success(&output);
    assert_eq!(stdout(&output), "<a b\nc>");
    assert_eq!(stderr(&output), "warning");
}

#[test]
fn lexical_functions_loops_and_typed_returns() {
    let output = run(r#"
        (let name outer)
        (defn show () (printf "%s\n" $name))
        (defn call (name) (show))
        (call inner)
        (set name changed)
        (call inner)
        (defn greet (who) (printf hello-) (printf "%s\n" $who))
        (printf "<%s>\n" (greet world))
        (defn items () (list "a b" c))
        (for item (items) (printf "[%s]\n" $item))
        (defn factory (value) (defn captured () (printf "%s" $value)))
        (factory retained)
        (captured)
    "#);
    success(&output);
    assert_eq!(
        stdout(&output),
        "outer\nchanged\n<hello-world>\n[a b]\n[c]\nretained"
    );
}

#[test]
fn conditions_check_status_without_disabling_fail_fast_elsewhere() {
    let output = run(
        "(if (false) (echo bad) (echo good)) (if (not (false)) (echo yes)) (if (eq a a) (echo equal))",
    );
    success(&output);
    assert_eq!(stdout(&output), "good\nyes\nequal\n");
    let output = run("(false) (echo bad)");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let output = run("(if (parsh-command-does-not-exist) (echo bad) (echo also-bad))");
    assert_eq!(output.status.code(), Some(127));
    assert!(output.stdout.is_empty());
}

#[test]
fn pipelines_stream_large_outputs_and_return_rightmost_failure() {
    let output = run("(| (head -c 1048576 /dev/zero) (cat))");
    success(&output);
    assert_eq!(output.stdout.len(), 1048576);
    let output = run("(printf \"<%s>\" (| (head -c 1048576 /dev/zero) (wc -c)))");
    success(&output);
    assert_eq!(stdout(&output), "<1048576>");
    let output = run(r#"(| (sh -c "exit 7") (sh -c "exit 3") (cat)) (echo bad)"#);
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let output = run("(| (sleep 30) (parsh-command-does-not-exist))");
    assert_eq!(output.status.code(), Some(127));
}

#[test]
fn redirections_compose_with_nested_capture_and_cwd() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(&format!(
        r#"
        (cd {})
        (|> out.txt (printf "%s" (printf "nested\n\n")))
        (|>> out.txt (printf "-tail"))
        (<| out.txt (cat))
        (|2> err.txt (sh -c "printf warning >&2"))
        (|> sorted.txt (| (printf "b\na\n") (sort)))
        (defn report () (sh -c "printf function-output; printf function-error >&2"))
        (|1> combined-out.txt (|2>> combined-err.txt (report)))
        (|1>> combined-out.txt
          (|2>> combined-err.txt
            (| (sh -c "printf pipeline-output; printf pipeline-error >&2") (cat))))
    "#,
        quote(directory.path().to_str().unwrap())
    ));
    success(&output);
    assert_eq!(stdout(&output), "nested-tail");
    assert_eq!(
        fs::read_to_string(directory.path().join("out.txt")).unwrap(),
        "nested-tail"
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("err.txt")).unwrap(),
        "warning"
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("sorted.txt")).unwrap(),
        "a\nb\n"
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("combined-out.txt")).unwrap(),
        "function-outputpipeline-output"
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("combined-err.txt")).unwrap(),
        "function-errorpipeline-error"
    );
}

#[test]
fn output_redirections_truncate_append_create_and_preserve_the_other_stream() {
    for (overwrite, append, redirects_stdout) in [
        ("|>", "|>>", true),
        ("|1>", "|1>>", true),
        ("|2>", "|2>>", false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("existing.txt"), "stale contents").unwrap();
        let output = run(&format!(
            r#"
            (cd {})
            ({overwrite} existing.txt (sh -c "printf out; printf err >&2"))
            ({append} existing.txt (sh -c "printf out; printf err >&2"))
            ({append} new.txt (sh -c "printf out; printf err >&2"))
            "#,
            quote(directory.path().to_str().unwrap())
        ));
        success(&output);
        let redirected = if redirects_stdout { "out" } else { "err" };
        assert_eq!(
            fs::read_to_string(directory.path().join("existing.txt")).unwrap(),
            redirected.repeat(2),
            "{overwrite} then {append}"
        );
        assert_eq!(
            fs::read_to_string(directory.path().join("new.txt")).unwrap(),
            redirected,
            "{append} creates a missing file"
        );
        assert_eq!(
            stdout(&output),
            if redirects_stdout { "" } else { "outoutout" }
        );
        assert_eq!(
            stderr(&output),
            if redirects_stdout { "errerrerr" } else { "" }
        );
    }
}

#[test]
fn glob_is_explicit_sorted_and_expands_lists() {
    let directory = tempfile::tempdir().unwrap();
    for name in ["b.rs", "a a.rs", ".hidden.rs", "c.txt"] {
        fs::write(directory.path().join(name), "").unwrap();
    }
    let output = run(&format!(
        r#"(cd {}) (printf "<%s>\n" (glob *.rs)) (printf "[%s]" *.rs) (for x (glob absent-*) (echo bad))"#,
        quote(directory.path().to_str().unwrap())
    ));
    success(&output);
    assert_eq!(stdout(&output), "<a a.rs>\n<b.rs>\n[*.rs]");
}

#[test]
fn export_updates_shell_environment_and_local_binding() {
    let output = run(
        r#"(let PARSH_TEST old) (export PARSH_TEST "a b") (printf "<%s>" $PARSH_TEST) (sh -c "printf '[%s]' \"$PARSH_TEST\"")"#,
    );
    success(&output);
    assert_eq!(stdout(&output), "<a b>[a b]");
}

#[test]
fn extra_closers_recover_and_are_reported_even_after_runtime_failure() {
    let output = run("(echo a))) (echo b)");
    assert_eq!(stdout(&output), "a\nb\n");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stderr(&output).matches("unexpected ')'").count(), 2);
    assert!(stderr(&output).contains("-c:1:9:"));
    assert!(stderr(&output).contains("-c:1:10:"));
    let output = run("(false))) (echo bad)");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(stderr(&output).matches("unexpected ')'").count(), 2);
}

#[test]
fn scripts_stdin_arguments_shebang_and_exit() {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("example.parsh");
    fs::write(
        &script,
        "#!/usr/bin/env parsh\n(printf \"<%s>\" $args) (cat)",
    )
    .unwrap();
    let output = invoke(&[script.to_str().unwrap(), "a b", "c"], "input");
    success(&output);
    assert_eq!(stdout(&output), "<a b><c>input");
    let output = invoke(&[], "(echo stdin)");
    success(&output);
    assert_eq!(stdout(&output), "stdin\n");
    let output = run("(do (exit 9) (echo bad)) (echo worse)");
    assert_eq!(output.status.code(), Some(9));
    assert!(output.stdout.is_empty());
}

#[test]
fn cli_help_version_and_argument_errors() {
    for flag in ["-h", "--help"] {
        let output = invoke(&[flag], "");
        success(&output);
        assert!(stderr(&output).is_empty());
        let help = stdout(&output);
        for text in ["Usage:", "SOURCE", "FILE", "--help", "--version", "Enter"] {
            assert!(help.contains(text), "missing {text:?}: {help}");
        }
    }
    for flag in ["-V", "--version"] {
        let output = invoke(&[flag], "");
        success(&output);
        assert_eq!(
            stdout(&output),
            concat!("parsh ", env!("CARGO_PKG_VERSION"), "\n")
        );
        assert!(stderr(&output).is_empty());
    }
    for args in [&["-c"][..], &["--parsh-unknown-option"]] {
        let output = invoke(args, "");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(stderr(&output).contains("error:"));
        assert!(stderr(&output).contains("--help"));
    }
}

#[test]
fn cli_passes_all_arguments_after_source_or_script_unchanged() {
    let source = r#"(printf "<%s>" $args)"#;
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("args.parsh");
    fs::write(&script, source).unwrap();
    let arguments = [
        "--help",
        "-h",
        "--version",
        "-V",
        "-c",
        "--",
        "--unknown",
        "a b",
        "",
    ];
    let expected = "<--help><-h><--version><-V><-c><--><--unknown><a b><>";
    for prefix in [&["-c", source][..], &[script.to_str().unwrap()], &["-"]] {
        let mut args = prefix.to_vec();
        args.extend(arguments);
        let output = invoke(&args, if prefix == ["-"] { source } else { "" });
        success(&output);
        assert_eq!(stdout(&output), expected, "{prefix:?}");
        assert!(stderr(&output).is_empty());
    }

    let output = invoke(&["-c", "", "--help"], "");
    success(&output);
    assert!(output.stdout.is_empty());
    let output = invoke(&["-c", "--help"], "");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("top-level expressions must be parenthesized"));
}

#[test]
fn cli_double_dash_allows_script_names_that_look_like_options() {
    let directory = tempfile::tempdir().unwrap();
    for name in ["--help", "-c"] {
        fs::write(directory.path().join(name), r#"(printf "<%s>" $args)"#).unwrap();
        let output = invoke_in(&["--", name, "--version"], "", Some(directory.path()));
        success(&output);
        assert_eq!(stdout(&output), "<--version>");
        assert!(stderr(&output).is_empty());
    }
}

#[test]
fn malformed_input_and_recursion_have_bounded_failures() {
    for source in [
        "(echo \"unclosed)",
        "(; unclosed",
        "(echo $missing)",
        "(defn f () (f)) (f)",
    ] {
        let output = run(source);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{source}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn variable_command_names_and_permission_errors() {
    let output = run(r#"(let program printf) ($program "<%s>" yes)"#);
    success(&output);
    assert_eq!(stdout(&output), "<yes>");
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("not-executable");
    fs::write(&file, "#!/bin/sh\necho bad\n").unwrap();
    let output = run(&format!("({})", quote(file.to_str().unwrap())));
    assert_eq!(output.status.code(), Some(126));
    let output = run("(defn ; () (echo bad))");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn binary_capture_fails_but_binary_redirection_preserves_bytes() {
    let output = run(r#"(printf "%s" (printf "\\377"))"#);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("not UTF-8"));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("binary");
    let output = run(&format!(
        r#"(|> {} (printf "\\377"))"#,
        quote(path.to_str().unwrap())
    ));
    success(&output);
    assert_eq!(fs::read(path).unwrap(), [0xff]);
}
