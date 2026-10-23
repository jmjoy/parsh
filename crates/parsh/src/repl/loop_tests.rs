use super::*;

fn values(events: &[Event]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Value(value) => Some(value.as_str()),
            _ => None,
        })
        .collect()
}

fn diagnostics(events: &[Event]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Diagnostic(message) => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

fn assert_values(source: &str, expected: &[&str]) {
    let events = execute(&mut Repl::new(), source);
    assert!(diagnostics(&events).is_empty(), "{source}: {events:?}");
    assert_eq!(values(&events), expected, "{source}: {events:?}");
}

#[test]
fn while_checks_the_condition_once_per_iteration_and_returns_the_last_body_value() {
    assert_values(
        r#"
        (let checks (+ 0))
        (let count (+ 0))
        (list (while (do (set checks (+ $checks 1)) (< $count 3))
                (set count (+ $count 1))
                (list $count $checks))
              $count $checks)
        (list (while (eq a b) $undefined))
        (while (< $count 1000) (set count (+ $count 1)) $count)
        "#,
        &["((3 3) 3 4)", "(nil)", "1000"],
    );
}

#[test]
fn while_iterations_have_fresh_lexical_scopes_retained_by_closures() {
    let mut repl = Repl::new();
    let events = execute(
        &mut repl,
        r#"
        (let name outer)
        (let count (+ 0))
        (while (< $count 2)
          (set count (+ $count 1))
          (let copy $name)
          (let name (str $copy $count))
          (if (= $count 1)
              (defn first () (list $copy $name))
              (defn second () (list $copy $name)))
          $name)
        (list (first) (second) $name)
        "#,
    );
    assert!(diagnostics(&events).is_empty(), "{events:?}");
    assert_eq!(
        values(&events),
        [
            r#""outer2""#,
            r#"(("outer" "outer1") ("outer" "outer2") "outer")"#
        ]
    );
    let events = execute(&mut repl, "(get copy)");
    assert!(diagnostics(&events)[0].contains("undefined variable $copy"));
}

#[test]
fn break_and_continue_preserve_the_last_complete_iteration_or_nil() {
    for (source, expected) in [
        ("(list (while (eq a a) (break)))", "(nil)"),
        ("(list (for item (list a b) (break)))", "(nil)"),
        ("(list (for item (list a b) (continue)))", "(nil)"),
        (
            "(for item (list first second third) (if (eq $item second) (break)) (str $item))",
            r#""first""#,
        ),
        (
            "(for item (list first second third) (if (not (eq $item first)) (continue)) (str $item))",
            r#""first""#,
        ),
        (
            "(let n (+ 0)) (while (< $n 3) (set n (+ $n 1)) (if (= $n 2) (break)) (str $n))",
            r#""1""#,
        ),
        (
            "(let n (+ 0)) (while (< $n 3) (set n (+ $n 1)) (if (> $n 1) (continue)) (str $n))",
            r#""1""#,
        ),
        (
            "(let n (+ 0)) (list (while (< $n 3) (set n (+ $n 1)) (continue)))",
            "(nil)",
        ),
    ] {
        assert_values(source, &[expected]);
    }
}

#[test]
fn break_values_preserve_types_and_are_evaluated_exactly_once() {
    for header in ["while (< $n 3)", "for item (list a b c)"] {
        for (value, expected) in [
            ("(+ 7)", "(7 2)"),
            ("(list a (+ 2))", r#"(("a" 2) 2)"#),
            ("(eq a a)", "(true 2)"),
            ("(do)", "(nil 2)"),
            ("text", r#"("text" 2)"#),
            ("(do (set n (+ $n 10)) (list $n))", "((12) 12)"),
        ] {
            let source = format!(
                "(let n (+ 0)) (list ({header} (set n (+ $n 1)) \
                 (if (= $n 2) (break {value})) (str completed)) $n)"
            );
            assert_values(&source, &[expected]);
        }
    }
}

#[test]
fn mixed_nested_loops_target_only_the_nearest_loop() {
    for outer in ["while (< $round 3)", "for outer_item (list a b c)"] {
        for inner in ["while (< $n 5)", "for inner_item (list a b c d e)"] {
            let source = format!(
                "(let round (+ 0)) (let visits (+ 0)) (let after (+ 0))
                 ({outer}
                   (set round (+ $round 1))
                   (let n (+ 0))
                   ({inner}
                     (set n (+ $n 1))
                     (if (= $n 2) (continue))
                     (if (= $n 4) (break (list inner-done)))
                     (set visits (+ $visits 1)))
                   (if (= $round 2) (continue))
                   (set after (+ $after 1)))
                 (list $visits $after $round)"
            );
            assert_values(&source, &["(6 2 3)"]);
        }
    }
}

#[test]
fn loop_jumps_unwind_nested_expressions_without_evaluating_the_rest() {
    for jump in ["(break returned)", "(continue)"] {
        for body in [
            format!("(do (str before) {jump} $undefined)"),
            format!("(if (eq a a) {jump} $undefined)"),
            format!("(str {jump} $undefined)"),
            format!("(list {jump} $undefined)"),
            format!("(+ 1 {jump} $undefined)"),
            format!("(= 1 {jump})"),
            format!("(not {jump})"),
            format!("(get {jump})"),
            format!("(let unused {jump})"),
            format!("(set n {jump})"),
            format!(r#"(str "before=${jump}")"#),
            format!(r#"("${jump}" $undefined)"#),
            format!("(unavailable {jump} $undefined)"),
            format!("(while {jump} $undefined)"),
            format!("(for item {jump} $undefined)"),
        ] {
            let source =
                format!("(let n (+ 0)) (list (while (< $n 3) (set n (+ $n 1)) {body}) $n)");
            let expected = if jump.starts_with("(break") {
                r#"("returned" 1)"#
            } else {
                "(nil 3)"
            };
            assert_values(&source, &[expected]);
        }
    }
}

#[test]
fn functions_isolate_loop_control_but_arguments_use_the_callers_loop() {
    for jump in ["(break)", "(continue)"] {
        let mut repl = Repl::new();
        let source = format!("(defn jump () {jump}) (for item (list a b) (jump))");
        let events = execute(&mut repl, &source);
        assert!(diagnostics(&events)[0].contains("in jump:"), "{events:?}");
        assert!(
            diagnostics(&events)[0].contains("outside a loop body"),
            "{events:?}"
        );
        let events = execute(&mut repl, jump);
        assert!(
            diagnostics(&events)[0].contains("outside a loop body"),
            "{events:?}"
        );
        assert_eq!(values(&execute(&mut repl, "(+ 1 2)")), ["3"]);
    }
    assert_values(
        "(defn inner () (for item (list a b) (break (list $item))))
         (let n (+ 0))
         (while (< $n 3)
           (set n (+ $n 1)) (inner)
           (if (= $n 1) (continue))
           (break $n))
         (defn unused (arg) (unavailable))
         (while (eq a a) (unused (break from-argument)))",
        &["2", r#""from-argument""#],
    );
}

#[test]
fn invalid_loops_and_jumps_report_positions_without_evaluating_arguments() {
    for (source, message, position) in [
        ("(while)", "expected at least 2 arguments", "1:1"),
        ("(while (eq a a))", "expected at least 2 arguments", "1:1"),
        (
            "(while\n (str text)\n (unavailable))",
            "a condition must be a boolean or command status",
            "2:2",
        ),
        (
            "(while\n (+ 1)\n (unavailable))",
            "a condition must be a boolean or command status",
            "2:2",
        ),
        (
            "(while\n (list)\n (unavailable))",
            "a condition must be a boolean or command status",
            "2:2",
        ),
        (
            "(while\n (do)\n (unavailable))",
            "a condition must be a boolean or command status",
            "2:2",
        ),
        (
            "(while (eq a a)\n (break a b))",
            "expected 0..1 arguments",
            "2:2",
        ),
        (
            "(for item (list a)\n (continue (set sentinel changed)))",
            "expected 0 arguments",
            "2:2",
        ),
        (
            "(break (set sentinel changed))",
            "break used outside a loop body",
            "1:1",
        ),
        ("(continue)", "continue used outside a loop body", "1:1"),
        (
            "(while\n (break (set sentinel changed))\n (unavailable))",
            "break used outside a loop body",
            "2:2",
        ),
        (
            "(for item\n (continue)\n (unavailable))",
            "continue used outside a loop body",
            "2:2",
        ),
        ("(while (eq a a)\n (+ 1 invalid))", "invalid number", "2:7"),
    ] {
        let mut repl = Repl::new();
        execute(&mut repl, "(let sentinel retained)");
        let events = execute(&mut repl, &format!("{source} (str skipped)"));
        let errors = diagnostics(&events);
        assert_eq!(errors.len(), 1, "{source}: {events:?}");
        assert!(errors[0].contains(message), "{source}: {events:?}");
        assert!(
            errors[0].starts_with(&format!("interactive:{position}:")),
            "{events:?}"
        );
        assert!(values(&events).is_empty(), "{events:?}");
        assert_eq!(repl.session.last_code, 2);
        assert_eq!(
            values(&execute(&mut repl, "(str $sentinel)")),
            [r#""retained""#]
        );
        assert!(diagnostics(&execute(&mut repl, "(break)"))[0].contains("outside a loop body"));
    }
    for name in ["while", "break", "continue"] {
        let events = execute(&mut Repl::new(), &format!("(defn {name} () (str invalid))"));
        assert!(diagnostics(&events)[0].contains("function name is reserved"));
    }
    assert_values(
        "(if (eq a b) (break)) (if (eq a b) (continue)) (str done)",
        &[r#""done""#],
    );
}

#[test]
fn exit_and_interrupts_unwind_loop_context_and_allow_later_execution() {
    fn form(source: &str) -> Expr {
        let Read::Form(expr) = Reader::new(source).read() else {
            panic!("invalid test source")
        };
        expr
    }
    let mut shell = Shell::syntax();
    let error = shell
        .execute(&form("(while (eq a a) (exit 7))"))
        .unwrap_err();
    assert!(error.exit);
    assert_eq!(error.code, 7);
    let error = shell.execute(&form("(continue)")).unwrap_err();
    assert!(error.message.contains("outside a loop body"));

    shell
        .interrupted
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let interrupted = shell.interrupted.clone();
    let thread = std::thread::spawn(move || {
        // execute clears the flag, so wait for execution before interrupting it.
        while interrupted.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::yield_now();
        }
        interrupted.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let error = shell
        .execute(&form("(while (eq a a) (continue))"))
        .unwrap_err();
    thread.join().unwrap();
    assert_eq!(error.code, 130);
    assert!(!error.exit);
    let error = shell.execute(&form("(break)")).unwrap_err();
    assert!(error.message.contains("outside a loop body"));
    assert_eq!(shell.execute(&form("(+ 1 2)")).unwrap().display(), "3");
}
