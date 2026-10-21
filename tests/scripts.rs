use std::{
    fs,
    io::{Read, Write},
    os::unix::process::CommandExt,
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_parsh"))
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
    let output = run(r#"(let $name "a b") (printf "<%s>\n" $name 123 true nil ; a;b)"#);
    success(&output);
    assert_eq!(stdout(&output), "<a b>\n<123>\n<true>\n<nil>\n<;>\n<a;b>\n");
    for source in [
        "(let name x)",
        "(set name x)",
        "(for item (list a) (echo bad))",
        "(defn f (name) (echo bad))",
        "(export NAME x)",
    ] {
        let output = run(source);
        assert_eq!(output.status.code(), Some(2), "{source}");
        assert!(stderr(&output).contains("$variable"), "{}", stderr(&output));
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
        (let $name outer)
        (defn show () (printf "%s\n" $name))
        (defn call ($name) (show))
        (call inner)
        (set $name changed)
        (call inner)
        (defn greet ($who) (printf hello-) (printf "%s\n" $who))
        (printf "<%s>\n" (greet world))
        (defn items () (list "a b" c))
        (for $item (items) (printf "[%s]\n" $item))
        (defn factory ($value) (defn captured () (printf "%s" $value)))
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
}

#[test]
fn glob_is_explicit_sorted_and_expands_lists() {
    let directory = tempfile::tempdir().unwrap();
    for name in ["b.rs", "a a.rs", ".hidden.rs", "c.txt"] {
        fs::write(directory.path().join(name), "").unwrap();
    }
    let output = run(&format!(
        r#"(cd {}) (printf "<%s>\n" (glob *.rs)) (printf "[%s]" *.rs) (for $x (glob absent-*) (echo bad))"#,
        quote(directory.path().to_str().unwrap())
    ));
    success(&output);
    assert_eq!(stdout(&output), "<a a.rs>\n<b.rs>\n[*.rs]");
}

#[test]
fn export_updates_shell_environment_and_local_binding() {
    let output = run(
        r#"(let $PARSH_TEST old) (export $PARSH_TEST "a b") (printf "<%s>" $PARSH_TEST) (sh -c "printf '[%s]' \"$PARSH_TEST\"")"#,
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
    let output = run(r#"(let $program printf) ($program "<%s>" yes)"#);
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
