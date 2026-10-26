use std::{
    fs,
    io::{Read, Write},
    os::unix::{fs::PermissionsExt, process::CommandExt},
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
            .replace('$', "\\$")
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
fn maps_capture_commands_and_expand_only_explicit_key_and_value_lists() {
    let output = run(r#"
        (let m (map (printf name) (printf 'parsh\n\n') count #2))
        (set m (mset $m empty (printf '')))
        (printf "<%s>\n" (mget $m name) (mget $m count) (mget $m empty))
        (printf "key=%s\n" (mkeys $m))
        (set m (mset $m name updated))
        (set m (mdel $m empty))
        (printf "value=%s\n" (mvals $m))
        "#);
    success(&output);
    assert_eq!(
        stdout(&output),
        "<parsh>\n<2>\n<>\nkey=name\nkey=count\nkey=empty\nvalue=updated\nvalue=2\n"
    );
    for argument in ["$m", "(list $m)"] {
        let output = run(&format!("(let m (map)) (printf %s {argument})"));
        assert_eq!(output.status.code(), Some(2));
        assert!(stderr(&output).contains("expected a scalar value, got a map"));
        assert!(stdout(&output).is_empty());
    }
}

#[test]
fn collection_sugar_captures_commands_and_expands_list_arguments() {
    let output = run(r#"
        []{}
        (let config {(printf name) (printf 'parsh\n\n') items [(printf 'a b') second]})
        (printf "<%s>\n" (mget $config name) (mget $config items))
        (if (eq [one {key [two]}] (list one (map key (list two))))
            (echo equivalent)
            (exit 9))
    "#);
    success(&output);
    assert_eq!(stdout(&output), "<parsh>\n<a b>\n<second>\nequivalent\n");
    assert!(stderr(&output).is_empty());
}

#[test]
fn type_reports_builtins_functions_and_external_commands_without_running_them() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("type-probe");
    fs::write(&executable, "#!/bin/sh\nprintf should-not-run\nexit 9\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let output = run(&format!(
        r#"
        (export PATH {})
        (defn type-function () (exit 9))
        (type type cd type-function type-probe)
        (type -t (list type type-function type-probe))
        (type)
        "#,
        quote(directory.path().to_str().unwrap())
    ));
    success(&output);
    assert_eq!(
        stdout(&output),
        format!(
            "type is a shell builtin\ncd is a shell builtin\ntype-function is a function\ntype-probe is {}\nbuiltin\nfunction\nfile\n",
            executable.display()
        )
    );
    assert!(stderr(&output).is_empty());
    let output = run("(defn type () (str overridden))");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("function name is reserved"));
}

#[test]
fn type_options_respect_lookup_precedence_and_path_order() {
    let directory = tempfile::tempdir().unwrap();
    for bin in ["first", "second"] {
        fs::create_dir(directory.path().join(bin)).unwrap();
        for name in ["type", "type-probe"] {
            let executable = directory.path().join(bin).join(name);
            fs::write(&executable, "#!/bin/sh\nexit 9\n").unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let output = invoke_in(
        &[
            "-c",
            r#"
        (export PATH first:second)
        (defn type-probe () (exit 9))
        (type -at type type-probe)
        (type -p type type-probe)
        (type -P type type-probe)
        (type -ap type type-probe)
        (type -f type-probe)
        (type -a type-probe)
        (type -pt type-probe)
        (type -tp type-probe)
        (type -Pt type-probe)
        "#,
        ],
        "",
        Some(directory.path()),
    );
    success(&output);
    assert_eq!(
        stdout(&output),
        concat!(
            "builtin\nfile\nfile\nfunction\nfile\nfile\n",
            "first/type\nfirst/type-probe\n",
            "first/type\nsecond/type\nfirst/type-probe\nsecond/type-probe\n",
            "type-probe is first/type-probe\n",
            "type-probe is a function\ntype-probe is first/type-probe\ntype-probe is second/type-probe\n",
            "function\nfile\n"
        )
    );
    assert!(stderr(&output).is_empty());
}

#[test]
fn type_uses_session_cwd_and_only_reports_executable_files() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("bin")).unwrap();
    fs::create_dir(directory.path().join("bin/directory")).unwrap();
    fs::write(directory.path().join("bin/data"), "data").unwrap();
    let executable = directory.path().join("bin/-probe");
    fs::write(&executable, "#!/bin/sh\nexit 9\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    symlink("-probe", directory.path().join("bin/link")).unwrap();
    let output = invoke_in(
        &[
            "-c",
            r#"
        (export PATH missing:bin)
        (type -p -- -probe)
        (type -p ./bin/-probe bin/link)
        (if (type -t data directory absent "") (exit 9))
        (cd bin)
        (export PATH "")
        (type -p -- -probe)
        (export PATH :missing:.)
        (type -ap -- -probe)
        "#,
        ],
        "",
        Some(directory.path()),
    );
    success(&output);
    assert_eq!(
        stdout(&output),
        "bin/-probe\n./bin/-probe\nbin/link\n./-probe\n./-probe\n./-probe\n"
    );
    assert!(stderr(&output).is_empty());
}

#[test]
fn type_processes_all_names_and_propagates_status_and_usage_errors() {
    let output = run("(type no-such-parsh-command type another-missing-command) (exit 9)");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout(&output), "type is a shell builtin\n");
    assert!(stderr(&output).contains("type: no-such-parsh-command: not found"));
    assert!(stderr(&output).contains("type: another-missing-command: not found"));
    let output = run(r#"
        (if (type -t no-such-parsh-command) (exit 9))
        (if (type -P cd) (exit 9))
        (if (type -p cd) (type -t type) (exit 9))
    "#);
    success(&output);
    assert_eq!(stdout(&output), "builtin\n");
    assert!(stderr(&output).is_empty());
    let output = run("(type -z type) (exit 9)");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("type: -z: invalid option"));
    assert!(output.stdout.is_empty());
}

#[test]
fn type_output_supports_capture_and_stdout_and_stderr_redirection() {
    let directory = tempfile::tempdir().unwrap();
    let output = invoke_in(
        &[
            "-c",
            r#"
        (printf "<%s>\n" (type -t type))
        (|> output (type type))
        (|>> output (type -t cd))
        (printf "<%s>\n" (|> explicit (type -t type)))
        (if (|2> errors (type no-such-parsh-command)) (exit 9))
    "#,
        ],
        "",
        Some(directory.path()),
    );
    success(&output);
    assert_eq!(stdout(&output), "<builtin>\n<>\n");
    assert!(stderr(&output).is_empty());
    assert_eq!(
        fs::read_to_string(directory.path().join("output")).unwrap(),
        "type is a shell builtin\nbuiltin\n"
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("explicit")).unwrap(),
        "builtin\n"
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("errors")).unwrap(),
        "type: no-such-parsh-command: not found\n"
    );
    let output = run("(| (type type) (cat))");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("pipeline stages must be external commands"));
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
fn interpolated_strings_capture_commands_without_splitting_arguments() {
    let output = run(r#"
        (let name "a b")
        (defn greet () (str "$name!"))
        (printf "<%s>\n" "$name $(+ #1 #2)" '$name\n'
            "text=$(printf "x\n\n")" "$(greet)" "nested=$(str "a" 'b')"
            "$(sh -c 'printf captured; printf warning >&2')")
    "#);
    success(&output);
    assert_eq!(
        stdout(&output),
        "<a b 3>\n<$name\\n>\n<text=x>\n<a b!>\n<nested=ab>\n<captured>\n"
    );
    assert_eq!(stderr(&output), "warning");
    let output = run(r#"(printf %s "$(false)") (echo skipped)"#);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn triple_quoted_strings_keep_multiline_command_arguments_intact() {
    let output = run(r#"
        (let name "a b")
        (printf "<%s>\n" """hello "$name"
sum=$(+ #1 #2)
capture=$(printf "x\n\n")""" ''''$missing' $(missing)\n
literal''')
    "#);
    success(&output);
    assert_eq!(
        stdout(&output),
        "<hello \"a b\"\nsum=3\ncapture=x>\n<'$missing' $(missing)\\n\nliteral>\n"
    );
    for delimiter in ["\"\"\"", "'''"] {
        let output = run(&format!("(printf %s {delimiter}unfinished"));
        assert_eq!(output.status.code(), Some(2));
        assert!(stderr(&output).contains("unclosed triple-"));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn interpolated_command_names_pipelines_and_redirection_paths_work() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("out ' $literal.txt");
    let output = run(&format!(
        r#"
        (let name "a b")
        (let command printf)
        ("$command" "<%s>\n" "$(get name)")
        (| ("$(get command)" %s "sum=$(+ #1 #2)") (cat))
        (let path {})
        (|> "$path" (printf %s "file=$(+ #2 #3)"))
        (cat "$path")
    "#,
        quote(path.to_str().unwrap())
    ));
    success(&output);
    assert_eq!(stdout(&output), "<a b>\nsum=3file=5");
    assert_eq!(fs::read_to_string(path).unwrap(), "file=5");
    let output = run(r#"(let command str) (| ("$command" no) (cat))"#);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("pipeline stages must be external commands"));
}

#[test]
fn arithmetic_composes_with_command_capture_functions_arguments_and_environment() {
    let output = run(r#"
        (let count (+ (int (printf "  0012 \n")) #3))
        (defn twice (n) (* $n #2))
        (set count (+ $count #1))
        (printf "<%s>\n" $count (twice $count) (/ $count #2)
            (list (+ #1 #2) (/ #5 #2)) (str result= (/ #8 #2)) 00123)
        (export PARSH_NUMBER (/ #8 #2))
        (printf "<%s>\n" $PARSH_NUMBER)
        (sh -c "printf '<%s>\\n' \"\$PARSH_NUMBER\"")
        (set PARSH_NUMBER (+ #1 #2))
        (sh -c "printf '<%s>\\n' \"\$PARSH_NUMBER\"")
        (if (= $count #16.0) (printf equal))
        "#);
    success(&output);
    assert_eq!(
        stdout(&output),
        "<16>\n<32>\n<8.0>\n<3>\n<2.5>\n<result=4.0>\n<00123>\n<4.0>\n<4.0>\n<3>\nequal"
    );
}

#[test]
fn numeric_comparisons_explicitly_convert_captured_output_and_drive_conditions() {
    let output = run(r#"
        (let count (+ (int (printf "  0012 \n")) #3))
        (defn enough (n) (>= $n #15.0))
        (printf "%s\n" (< (int (printf 2)) (int (printf 10)))
            (<= $count #15) (> $count #14.5) (enough $count)
            (!= $count #15.0) (> #9007199254740993 #9007199254740992.0))
        (if (> $count #10) (printf larger) (echo skipped))
        (if (< $count #10) (echo skipped) (printf smaller))
        "#);
    success(&output);
    assert_eq!(
        stdout(&output),
        "true\ntrue\ntrue\ntrue\nfalse\ntrue\nlargersmaller"
    );
}

#[test]
fn numeric_results_are_not_implicitly_printed_in_scripts() {
    let output =
        run("(+ #1 #2) (/ #8 #2) (= #001 #1) (!= #1 #2) (< #1 #2) (<= #2 #2) (> #2 #1) (>= #2 #2)");
    success(&output);
    assert!(output.stdout.is_empty());
}

#[test]
fn numeric_failures_have_language_error_status_and_stop_the_script() {
    for (source, message) in [
        ("(printf %s (+ #1 invalid))", "expected a number"),
        ("(/ #1 #0)", "division by zero"),
        ("(+ #9223372036854775807 #1)", "integer overflow"),
        ("(+ #9223372036854775808)", "integer out of range for i64"),
        ("(* #1e308 #1e308)", "number must be finite"),
        ("(= #1 (list 1))", "expected a number"),
        ("(< #1 invalid)", "expected a number"),
        ("(<= #1)", "expected 2 arguments"),
        ("(> #1 (list 2))", "expected a number"),
        ("(>= #1 #1e309)", "number must be finite"),
        ("(!= #1 #2 #3)", "expected 2 arguments"),
        (
            "(| (+ #1 #2) (cat))",
            "pipeline stages must be external commands",
        ),
        (
            "(| (> #2 #1) (cat))",
            "pipeline stages must be external commands",
        ),
    ] {
        let output = run(&format!("{source} (echo skipped)"));
        assert_eq!(
            output.status.code(),
            Some(2),
            "{source}: {}",
            stderr(&output)
        );
        assert!(output.stdout.is_empty(), "{source}: {}", stdout(&output));
        assert!(
            stderr(&output).contains(message),
            "{source}: {}",
            stderr(&output)
        );
    }
    let output = run("(+ #1\n invalid)");
    assert!(
        stderr(&output).starts_with("-c:2:2:"),
        "{}",
        stderr(&output)
    );
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
fn while_command_conditions_and_loop_values_capture_complete_stdout() {
    let output = run(r#"
        (let n #0)
        (printf "<%s>\n"
          (while (test $n -lt 3)
            (printf "[%s]" $n)
            (set n (+ $n #1))
            (printf .)))
        (while (false) (echo skipped))
        (while (| (false) (true)) (echo skipped))
        (printf "<%s>\n"
          (while #true
            (printf discarded)
            (break (printf "a b\n\n"))))
        (printf "<%s>\n"
          (for item (list a b c)
            (printf discarded)
            (break (printf "c d\n\n"))))
        (echo done)
    "#);
    success(&output);
    assert_eq!(stdout(&output), "<[0].[1].[2].>\n<a b>\n<c d>\ndone\n");
}

#[test]
fn loop_jumps_propagate_through_host_arguments_and_redirection_paths() {
    for expression in [
        "(glob (break passed))",
        "(cd (break passed))",
        "(export UNUSED (break passed))",
        "(|> (break passed) (echo skipped))",
        "(| (printf (break passed) $undefined) (cat))",
    ] {
        let output = run(&format!(
            "(printf \"<%s>\" (while #true {expression} (echo skipped)))"
        ));
        success(&output);
        assert_eq!(stdout(&output), "<passed>", "{expression}");
    }

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("loop.txt");
    let output = run(&format!(
        r#"
        (|> {}
          (for item (list a b c)
            (if (eq $item b) (continue))
            (printf %s $item)
            (if (eq $item c) (break))
            (printf .)))
        (|>> {} (while #true (printf D) (break)))
        (cat {})
        "#,
        quote(path.to_str().unwrap()),
        quote(path.to_str().unwrap()),
        quote(path.to_str().unwrap()),
    ));
    success(&output);
    assert_eq!(stdout(&output), "a.cD");
    assert_eq!(fs::read_to_string(path).unwrap(), "a.cD");
}

#[test]
fn loop_errors_and_exit_preserve_status_and_stop_the_script() {
    for (source, code, message) in [
        ("(while #true (false))", 1, "status 1"),
        (
            "(if (while #true (false)) (echo skipped) (echo also-skipped))",
            1,
            "status 1",
        ),
        ("(while #true (break (false)))", 1, "status 1"),
        (
            "(while (parsh-command-does-not-exist) (echo skipped))",
            127,
            "parsh-command-does-not-exist",
        ),
        (
            "(while #true (parsh-command-does-not-exist))",
            127,
            "parsh-command-does-not-exist",
        ),
        ("(while #true (exit 9))", 9, ""),
        ("(for item (list a b) (exit 7))", 7, ""),
        ("(break (echo skipped))", 2, "outside a loop body"),
        (
            "(for item (list a) (continue (echo skipped)))",
            2,
            "expected 0 arguments",
        ),
    ] {
        let output = run(&format!("{source} (echo skipped)"));
        assert_eq!(
            output.status.code(),
            Some(code),
            "{source}: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains(message),
            "{source}: {}",
            stderr(&output)
        );
        assert!(output.stdout.is_empty(), "{source}: {}", stdout(&output));
    }

    let output = run("(if (for item (list only) (false)) (echo skipped) (echo expected))");
    success(&output);
    assert_eq!(stdout(&output), "expected\n");
}

#[test]
fn loop_builtins_are_rejected_as_pipeline_stages() {
    for stage in ["(while #false (echo skipped))", "(break)", "(continue)"] {
        let output = run(&format!("(| {stage} (cat)) (echo skipped)"));
        assert_eq!(output.status.code(), Some(2));
        assert!(stderr(&output).contains("pipeline stages must be external commands"));
        assert!(output.stdout.is_empty());
    }
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
        r#"(let PARSH_TEST old) (export PARSH_TEST "a b") (printf "<%s>" $PARSH_TEST) (sh -c "printf '[%s]' \"\$PARSH_TEST\"")"#,
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
    let output = run("[a]]} (echo recovered)");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "recovered\n");
    assert!(stderr(&output).contains("-c:1:4: unexpected ']'"));
    assert!(stderr(&output).contains("-c:1:5: unexpected '}'"));
    let output = run("(false)]} (echo bad)");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("unexpected ']'"));
    assert!(stderr(&output).contains("unexpected '}'"));
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
    let source = r#"(if (eq $args (list "--help" "-h" "--version" "-V" "-c" "--" "--unknown" "a b" "")) (exit 0) (exit 1))"#;
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
    for prefix in [&["-c", source][..], &[script.to_str().unwrap()], &["-"]] {
        let mut args = prefix.to_vec();
        args.extend(arguments);
        let output = invoke(&args, if prefix == ["-"] { source } else { "" });
        success(&output);
        assert!(output.stdout.is_empty(), "{prefix:?}");
        assert!(stderr(&output).is_empty());
    }

    let output = invoke(&["-c", "", "--help"], "");
    success(&output);
    assert!(output.stdout.is_empty());
    let output = invoke(&["-c", "--help"], "");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output)
            .contains("top-level expressions must be calls, collection expressions or literals")
    );
}

#[test]
fn cli_double_dash_allows_script_names_that_look_like_options() {
    let directory = tempfile::tempdir().unwrap();
    for name in ["--help", "-c"] {
        fs::write(
            directory.path().join(name),
            r#"(if (eq $args (list "--version")) (exit 0) (exit 1))"#,
        )
        .unwrap();
        let output = invoke_in(&["--", name, "--version"], "", Some(directory.path()));
        success(&output);
        assert!(output.stdout.is_empty());
        assert!(stderr(&output).is_empty());
    }
}

#[test]
fn malformed_input_and_recursion_have_bounded_failures() {
    for source in [
        "(echo \"unclosed)",
        "(; unclosed",
        "[a",
        "{key value",
        "[a}",
        "{key value)",
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

#[test]
fn structs_and_macros_run_in_scripts() {
    let output = run(r#"
        (defstruct Point x y)
        (let point (Point 3 4))
        (printf "x=%s y=%s\n" (Point.x $point) (Point.y $point))
        (if (Point? $point) (echo is-point))
        (defmacro unless (condition body)
          `(if (not ,$condition) ,$body))
        (unless (eq 1 2) (echo taken))
        (defmacro begin (body) `(do ,@$body))
        (begin ((echo one) (echo two)))
        (type Point Point.x Point?)
    "#);
    success(&output);
    assert_eq!(
        stdout(&output),
        "x=3 y=4\nis-point\ntaken\none\ntwo\n\
         Point is a struct\n\
         Point.x is a struct accessor\n\
         Point? is a struct predicate\n"
    );
    assert!(stderr(&output).is_empty());
}

#[test]
fn struct_and_macro_failures_stop_scripts_with_language_status() {
    let output = run("(defstruct Point x) (Point 1 2)");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("expected 1 arguments, got 2"));
    let output = run("(defmacro bad () 1) (bad)");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("macro bad must return a quoted form"));
    let output = run("(unquote (str x))");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unquote is only valid inside a quasiquote"));
}

#[test]
fn logical_functions_handle_command_status_and_preserve_fail_fast_bodies() {
    let output = run(r#"
        (if (and (true) (not (false))) (printf and-ok))
        (if (or (false) (true)) (printf or-ok))
        (let invert (function not))
        (if ($invert (false)) (printf ref-ok))
        (defn optional (&optional (x (false))) $x)
        (if (and (false) (optional)) (printf unreachable))
        (if (or (true) (optional)) (printf skipped))
    "#);
    success(&output);
    assert_eq!(stdout(&output), "and-okor-okref-okskipped");
    let output = run("(defn fail () (false) (true)) (if (or (fail) (true)) (echo wrong))");
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
}

#[test]
fn function_values_capture_command_output_and_return_across_redirections() {
    let directory = tempfile::tempdir().unwrap();
    let output = invoke_in(
        &[
            "-c",
            r#"
        (defn captured ()
          (printf ignored)
          (return (printf returned))
          (exit 9))
        (printf "<%s>\n" (captured) ((function captured))
                (apply (function captured) []))
        (defn redirected () (|> output.txt (return (list one two))) (exit 9))
        (printf "<%s>\n" (redirected))
        (apply (function cd) [.])
        (printf "<%s>\n" (apply (function glob) ['*.txt']))
        (printf "<%s>\n" (apply (function type) [-t fn lget round]))
        (printf "<%s>\n" (sjoin (slines (printf ' a\nb\n')) ','))
    "#,
        ],
        "",
        Some(directory.path()),
    );
    success(&output);
    assert_eq!(
        stdout(&output),
        "<returned>\n<returned>\n<returned>\n<one>\n<two>\n<output.txt>\n<builtin\nbuiltin\nbuiltin>\n< a,b>\n"
    );
    assert!(
        fs::read(directory.path().join("output.txt"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn eager_defaults_fail_even_when_the_requested_entry_exists() {
    for source in [
        "(mget {key value} key (sh -c 'exit 7')) (echo wrong)",
        "(lget [value] #0 (sh -c 'exit 7')) (echo wrong)",
        "((function mget) {key value} key (sh -c 'exit 7')) (echo wrong)",
        "(apply (function lget) [[value] #0 (sh -c 'exit 7')]) (echo wrong)",
    ] {
        let output = run(source);
        assert_eq!(
            output.status.code(),
            Some(7),
            "{source}: {}",
            stderr(&output)
        );
        assert!(stdout(&output).is_empty());
    }
}

#[test]
fn scalar_literals_and_conversions_preserve_external_text_and_failure_status() {
    let output = run(r##"
        #1 #false #nil
        (printf "%s|%s|%s|%s\n" #1 #1.0 #false #nil)
        (printf "%s|%s|%s|%s\n" (int (printf ' 1e2 ')) (float 1)
                (bool (printf ' false ')) (typeof (printf '#1')))
        #false
        (printf continued)
    "##);
    success(&output);
    assert_eq!(
        stdout(&output),
        "1|1.0|false|\n100|1.0|false|text\ncontinued"
    );
    let output = run("#false (false) (printf skipped)");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    for source in [
        "#flase",
        "(int #1.5)",
        "(float \"1e309\")",
        "(bool #1)",
        "(#1 (printf skipped))",
    ] {
        let output = run(source);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{source}: {}",
            stderr(&output)
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn numeric_text_from_commands_environment_and_arguments_requires_conversion() {
    for source in [
        "(+ (printf 1) #1)",
        "(export NUMBER_TEXT 1) (+ $NUMBER_TEXT #1)",
        "(lget [a b] (printf 0))",
    ] {
        let output = run(source);
        assert_eq!(output.status.code(), Some(2), "{source}");
        assert!(
            stderr(&output).contains("got text"),
            "{source}: {}",
            stderr(&output)
        );
        assert!(output.stdout.is_empty());
    }
    let output = invoke(&["-c", "(+ (lfirst $args) #1)", "1"], "");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("int/float to convert text"));
    let output = invoke(&["-c", "(printf %s (+ (int (lfirst $args)) #1))", "1"], "");
    success(&output);
    assert_eq!(stdout(&output), "2");
}

#[test]
fn complete_results_preserve_both_streams_status_and_last_result() {
    let output = run(r#"
        (let here (pwd)) (let saved $?)
        (printf "%s/%s\n" (exit-code $saved) (eq $here (strim (stdout-text $saved))))
        (let r (capture-call sh -c 'printf output; printf error >&2; exit 7'))
        (set saved $?)
        (printf "%s/%s/%s/%s/%s\n" (stdout-text $r) (stderr-text $r)
                (exit-code $r) (signal $r) (eq $r $saved))
        (try (check $r) (catch e (printf "%s/%s" (Error.kind $e) (exit-code (Error.result $e)))))
        "#);
    success(&output);
    assert_eq!(
        stdout(&output),
        "0/true\noutput/error/7//true\ncommand-failed/7"
    );
}

#[test]
fn commands_through_apply_preserve_argument_values_and_run_once() {
    let output = run(r#"
        (let calls #0)
        (defn once () (set calls (+ $calls #1)) '$missing (exit 99)')
        (apply (command printf) '<%s>' [(once) 'a b'])
        (printf "/%s/" $calls)
        (apply printf ['<%s>' hello world])
        (defn echo (x) (str wrapped- $x))
        (printf "/%s/%s" (capture-call echo a) (stdout-text (capture-call (command echo) a)))
        "#);
    // A result struct cannot accidentally become an external argument.
    assert_eq!(output.status.code(), Some(2));
    assert!(stdout(&output).starts_with("<$missing (exit 99)><a b>/1/<hello><world>"));
    assert!(stderr(&output).contains("got a struct"));
    let output = run(r#"(defn echo (x) (str wrapped- $x))
        (printf "%s/%s" (CallResult.value (capture-call echo a))
                (strim (stdout-text (capture-call (command echo) a))))"#);
    success(&output);
    assert_eq!(stdout(&output), "wrapped-a/a");
}

#[test]
fn signal_termination_is_distinct_from_an_explicit_exit_code() {
    let output = run(r#"
        (let normal (capture-call sh -c 'exit 143'))
        (let killed (capture-call sh -c 'kill -TERM $$'))
        (printf "%s/%s/%s/%s" (exit-code $normal) (signal $normal)
                (exit-code $killed) (signal $killed))
        "#);
    success(&output);
    assert_eq!(stdout(&output), "143///15");
}

#[test]
fn raw_capture_accepts_binary_and_dual_streams_without_deadlocking() {
    let output = run(r#"
        (let binary (capture-call sh -c 'printf "\377"'))
        (check $binary)
        (printf "%s/%s/" (bytes? (CallResult.stdout $binary))
                (eq (CallResult.stdout $binary) (CallResult.stdout (capture-call sh -c 'printf "\377"'))))
        (try (stdout-text $binary) (catch e (printf invalid)))
        (let large (capture-call sh -c 'head -c 1048576 /dev/zero; head -c 1048576 /dev/zero >&2'))
        (printf "/%s/%s" (slen (stdout-text $large)) (slen (stderr-text $large)))
        "#);
    success(&output);
    assert_eq!(stdout(&output), "true/true/invalid/1048576/1048576");
}

#[test]
fn capture_limits_raise_an_error_with_partial_output() {
    let output = run(r#"
        (try (capture-call head -c 8388609 /dev/zero)
          (catch e (printf "%s/%s" (Error.kind $e)
            (bytes? (CallResult.stdout (Error.result $e))))))
        "#);
    success(&output);
    assert_eq!(stdout(&output), "output-limit/true");
}

#[test]
fn capture_preserves_redirection_and_pipeline_stage_information() {
    let directory = tempfile::tempdir().unwrap();
    let output = invoke_in(
        &[
            "-c",
            r#"
        (let r (|> out (|2> err (capture-call sh -c 'printf stdout; printf stderr >&2'))))
        (printf "%s/%s/" (nil? (CallResult.stdout $r)) (nil? (CallResult.stderr $r)))
        (cat out err)
        (set r (capture-call (fn () (| (sh -c 'exit 7') (sh -c 'exit 3') (cat)))))
        (printf "/%s/%s/%s" (exit-code $r) (llen (CallResult.stages $r))
          (Exited.code (StageResult.termination (lfirst (CallResult.stages $r)))))
        "#,
        ],
        "",
        Some(directory.path()),
    );
    success(&output);
    assert_eq!(stdout(&output), "true/true/stdoutstderr/3/3/7");
}

#[test]
fn captured_function_failures_stop_the_body_and_finally_runs() {
    let output = run(r#"
        (let trace "")
        (defn failing ()
          (try (printf before) (false) (set trace wrong)
            (finally (set trace cleaned))))
        (let r (capture-call failing))
        (printf "%s/%s/%s" (stdout-text $r) (exit-code $r) $trace)
        (try (capture-call missing-parsh-command) (catch e (printf "/%s/%s" (Error.kind $e) (nil? (CallResult.termination $?)))))
        "#);
    success(&output);
    assert_eq!(stdout(&output), "before/1/cleaned/spawn-failed/true");
}

#[test]
fn background_commands_capture_output_and_wait_publishes_the_result() {
    let output = run(r#"
        (let j (spawn sh -c 'printf hello; printf warning >&2; exit 3'))
        (let launched $?)
        (let r (wait $j)) (let completed $?)
        (printf "%s/%s/%s/%s/%s/%s/%s" (eq (CallResult.value $launched) $j)
          (nil? (CallResult.termination $launched)) (stdout-text $r) (stderr-text $r)
          (exit-code $r) (eq $r $completed) (eq $r (wait $j)))
        (try (check $r) (catch e (printf "/%s" (Error.kind $e))))
        "#);
    success(&output);
    assert_eq!(
        stdout(&output),
        "true/true/hello/warning/3/true/true/command-failed"
    );
}

#[test]
fn background_functions_copy_closure_graphs_and_return_stable_closures() {
    let output = run(r#"
        (let value parent)
        (let recurse (fn (n) (if (= $n #0) $value ($recurse (- $n #1)))))
        (let job (spawn (fn () (set value child) ($recurse #3))))
        (let r (wait $job))
        (printf "%s/%s/" $value (CallResult.value $r))
        (set job (spawn (fn () (let offset #5) (fn (x) (+ $offset $x)))))
        (let first (wait $job)) (let second (wait $job))
        (let f (CallResult.value $first))
        (printf "%s/%s" ($f #2) (eq $f (CallResult.value $second)))
        "#);
    success(&output);
    assert_eq!(stdout(&output), "parent/child/7/true");
}

#[test]
fn background_builtins_and_environment_changes_are_isolated() {
    let directory = tempfile::tempdir().unwrap();
    let output = invoke_in(
        &[
            "-c",
            r#"
        (export PARSH_JOB_VALUE parent)
        (let original (pwd))
        (let j (spawn (fn () (cd /) (export PARSH_JOB_VALUE child) (printf "%s/%s" (pwd) $PARSH_JOB_VALUE))))
        (let r (wait $j))
        (printf "%s/%s/%s/" (stdout-text $r) (eq $original (pwd)) $PARSH_JOB_VALUE)
        (let math (spawn + #2 #3))
        (printf "%s/" (CallResult.value (wait $math)))
        (let empty (spawn cat))
        (printf "%s" (eq (stdout-text (wait $empty)) ""))
        "#,
        ],
        "",
        Some(directory.path()),
    );
    success(&output);
    assert_eq!(stdout(&output), "//child/true/parent/5/true");
}

#[test]
fn background_errors_and_cancellation_are_results() {
    let output = run(r#"
        (let failed (spawn (fn () (throw {reason expected}))))
        (let r (wait $failed))
        (printf "%s/" (Error.kind (CallResult.error $r)))
        (let process (spawn sleep 30)) (cancel $process)
        (set r (wait $process))
        (printf "%s/" (Error.kind (CallResult.error $r)))
        (let loop (spawn (fn () (while #true #nil)))) (cancel $loop)
        (set r (wait $loop))
        (printf "%s/" (Error.kind (CallResult.error $r)))
        (try (spawn missing-parsh-command) (catch e (printf "%s" (Error.kind $e))))
        "#);
    success(&output);
    assert_eq!(stdout(&output), "user/cancelled/cancelled/spawn-failed");
}

#[test]
fn session_exit_cancels_background_processes_and_interpreter_loops() {
    let directory = tempfile::tempdir().unwrap();
    let output = invoke_in(
        &[
            "-c",
            r#"
        (let j (spawn sh -c 'printf "%s" $$ > pid; exec sleep 30'))
        (while (not (test -f pid)) (sleep 0.01))
        (spawn (fn () (while #true #nil)))
        (exit 0)
        "#,
        ],
        "",
        Some(directory.path()),
    );
    success(&output);
    let pid: i32 = fs::read_to_string(directory.path().join("pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
        Err(nix::errno::Errno::ESRCH)
    );
}

#[test]
fn finally_executes_before_exit_and_during_cooperative_cancellation() {
    let directory = tempfile::tempdir().unwrap();
    let output = invoke_in(
        &[
            "-c",
            r#"
        (defn cleanup () (|> cleaned (printf first)) (|>> cleaned (printf second)))
        (let j (spawn (fn ()
          (try (|> ready (printf ready)) (while #true #nil)
            (finally (cleanup))))))
        (while (not (test -f ready)) (sleep 0.01))
        (cancel $j) (wait $j)
        (try (exit 7) (finally (|> exited (printf yes))))
        "#,
        ],
        "",
        Some(directory.path()),
    );
    assert_eq!(output.status.code(), Some(7), "{}", stderr(&output));
    assert_eq!(
        fs::read_to_string(directory.path().join("cleaned")).unwrap(),
        "firstsecond"
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("exited")).unwrap(),
        "yes"
    );
}

#[test]
fn task_exit_and_background_completion_do_not_exit_or_overwrite_the_parent() {
    let output = run(r#"
        (let j (spawn (fn () (exit 9))))
        (let launched $?)
        (while (eq (JobStatus.state (job-status $j)) running) #nil)
        (let unchanged (eq $? $launched))
        (let r (wait $j))
        (printf "%s/%s/" $unchanged (exit-code $r))
        (try (check $r) (catch e (printf checked)))
        "#);
    success(&output);
    assert_eq!(stdout(&output), "true/9/checked");
}

#[test]
fn finally_and_skipped_iterations_cannot_replace_the_returned_command_metadata() {
    let output = run(r#"
        (let r (capture-call (fn () (try (sh -c 'exit 7') (finally (printf cleaned))))))
        (printf "%s/%s/" (exit-code $r) (stdout-text $r))
        (let next (capture-call (fn ()
          (for n [#0 #1]
            (if (= $n #1) (do (printf skipped) (continue)))
            (printf completed)))))
        (printf "%s/%s" (stdout-text $next)
          (lfirst (StageResult.argv (lfirst (CallResult.stages $next)))))
        "#);
    success(&output);
    assert_eq!(stdout(&output), "7/cleaned/completedskipped/printf");
}

#[test]
fn checking_a_cancelled_child_is_catchable_without_cancelling_the_parent() {
    let output = run(r#"
        (let job (spawn sleep 30)) (cancel $job)
        (let r (wait $job))
        (try (check $r) (catch e (printf "%s" (Error.kind $e))))
        (printf /parent-alive)
        "#);
    success(&output);
    assert_eq!(stdout(&output), "cancelled/parent-alive");
}

#[test]
fn external_references_and_dynamic_text_are_valid_pipeline_heads() {
    let output = run(r#"
        (let sorter (command sort))
        (let sink cat)
        (let r (capture-call (fn () (| (printf "b\na\n") ($sorter) ($sink)))))
        (printf "%s" (stdout-text $r))
        (let j (spawn (fn () (| (printf worker) ((command cat))))))
        (printf "%s" (stdout-text (wait $j)))
        "#);
    success(&output);
    assert_eq!(stdout(&output), "a\nb\nworker");
}
