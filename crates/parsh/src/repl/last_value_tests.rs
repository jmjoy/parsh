use super::*;

fn submit(repl: &mut Repl, source: &str) -> Vec<String> {
    let events = execute(repl, source);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(_))),
        "{source}: {events:?}"
    );
    events
        .into_iter()
        .filter_map(|event| match event {
            Event::Value(value) => Some(value),
            _ => None,
        })
        .collect()
}

fn binding(repl: &mut Repl, name: &str) -> Value {
    let source = format!("(get {name})");
    let Read::Form(expr) = Reader::new(&source).read() else {
        panic!("{source}")
    };
    repl.shell.execute(&expr).unwrap()
}

#[test]
fn previous_value_updates_between_submissions_and_each_top_level_expression() {
    let mut repl = Repl::new();
    assert_eq!(submit(&mut repl, "(+ #1 #2)"), ["#3"]);
    assert_eq!(submit(&mut repl, "(+ $_ #4)"), ["#7"]);
    assert_eq!(
        submit(&mut repl, "(+ $_ #1) (+ $_ #2) (get _)"),
        ["#8", "#10", "#10"]
    );
    assert_eq!(submit(&mut repl, "(do (+ #100 #200) (+ $_ #1))"), ["#11"]);
}

#[test]
fn previous_value_preserves_data_types_and_callable_functions() {
    for (source, kind, display) in [
        ("#42", "int", "#42"),
        ("#1.5", "float", "#1.5"),
        ("#true", "bool", "#true"),
        ("(str 'hello $_')", "text", r#""hello \$_""#),
        (
            "[#1 #false {key value}]",
            "list",
            r#"(#1 #false (map "key" "value"))"#,
        ),
        ("{enabled #true}", "map", r#"(map "enabled" #true)"#),
        ("(quote (+ #1 #2))", "form", "(+ #1 #2)"),
        ("(bytes/from-text abc)", "bytes", "<bytes 3>"),
        ("(defstruct Point x) (Point #2)", "struct", "(Point #2)"),
    ] {
        let mut repl = Repl::new();
        assert_eq!(submit(&mut repl, source), [display], "{source}");
        let previous = binding(&mut repl, "_");
        assert_eq!(previous.type_name(), kind, "{source}");
        assert_eq!(submit(&mut repl, "(get _)"), [display], "{source}");
        assert_eq!(binding(&mut repl, "_"), previous, "{source}");
    }

    let mut repl = Repl::new();
    submit(&mut repl, "(fn (x) (+ $x #1))");
    assert!(matches!(binding(&mut repl, "_"), Value::Function(_)));
    assert_eq!(submit(&mut repl, "($_ #41)"), ["#42"]);
}

#[test]
fn nil_returns_replace_previous_values_and_assignments_without_changing_saved_values() {
    let mut repl = Repl::new();
    submit(&mut repl, "[#1 {enabled #true}]");
    let saved = binding(&mut repl, "_");
    assert!(submit(&mut repl, "(let saved $_)").is_empty());
    assert_eq!(binding(&mut repl, "_"), Value::Nil);
    assert_eq!(binding(&mut repl, "saved"), saved);
    assert!(submit(&mut repl, "(set _ #42)").is_empty());
    assert_eq!(binding(&mut repl, "_"), Value::Nil);
    submit(&mut repl, "#true");
    assert!(submit(&mut repl, "#nil").is_empty());
    assert_eq!(binding(&mut repl, "_"), Value::Nil);
    assert_eq!(binding(&mut repl, "saved"), saved);
}

#[test]
fn underscore_follows_ordinary_assignment_export_and_shadowing_rules() {
    let mut repl = Repl::new();
    assert_eq!(submit(&mut repl, "(do (set _ #41) (+ $_ #1))"), ["#42"]);
    assert_eq!(
        submit(&mut repl, "(do (let _ #7) (set _ (+ $_ #1)) $_)"),
        ["#8"]
    );
    assert_eq!(submit(&mut repl, "((fn (_) (+ $_ #1)) #9)"), ["#10"]);
    assert_eq!(submit(&mut repl, "(for _ [#3 #4] $_)"), ["#4"]);
    assert_eq!(
        submit(&mut repl, "(do (export _ exported) $_)"),
        [r#""exported""#]
    );
    let events = execute(&mut repl, "(let _ #100)");
    assert!(
        events.iter().any(
            |event| matches!(event, Event::Diagnostic(text) if text.contains("already bound"))
        )
    );
    assert_eq!(binding(&mut repl, "_"), Value::Text("exported".into()));
}

#[test]
fn empty_comments_incomplete_input_and_errors_do_not_automatically_replace_previous_value() {
    let mut repl = Repl::new();
    submit(&mut repl, "#5");
    for source in [
        " \n\t",
        "(; ignored)",
        "(+ #100 #1) (str 'unfinished",
        "(+ #100 #1) ]",
        "(+ #1 text)",
        "(do (+ #100 #200) (throw stop))",
    ] {
        execute(&mut repl, source);
        assert_eq!(binding(&mut repl, "_").display(), "#5", "{source}");
        repl.input(Key::ClearInput);
    }
}

#[test]
fn runtime_failures_keep_completed_results_and_do_not_roll_back_explicit_assignments() {
    let mut repl = Repl::new();
    let events = execute(&mut repl, "(+ #1 #2) (throw stop) (+ #100 #200)");
    assert!(events.contains(&Event::Value("#3".into())));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(_)))
    );
    assert_eq!(binding(&mut repl, "_").display(), "#3");
    let events = execute(&mut repl, "(do (set _ #42) (throw stop))");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(_)))
    );
    assert_eq!(binding(&mut repl, "_").display(), "#42");
}

#[test]
fn previous_values_are_isolated_and_reset_to_nil() {
    let mut first = Repl::new();
    let mut second = Repl::new();
    assert_eq!(binding(&mut first, "_"), Value::Nil);
    assert_eq!(binding(&mut second, "_"), Value::Nil);
    submit(&mut first, "#7");
    assert_eq!(binding(&mut second, "_"), Value::Nil);
    first.reset();
    assert_eq!(binding(&mut first, "_"), Value::Nil);
    assert_eq!(submit(&mut first, "(nil? $_)"), ["#true"]);
}

#[test]
fn automatic_previous_value_updates_leave_last_execution_result_unchanged() {
    let mut repl = Repl::new();
    assert_eq!(binding(&mut repl, "?"), Value::Nil);
    submit(&mut repl, "(str hello)");
    assert_eq!(binding(&mut repl, "?"), Value::Nil);
    submit(&mut repl, "(defn answer () #42) (answer)");
    let result = binding(&mut repl, "?");
    assert!(matches!(result, Value::Struct { .. }));
    assert_eq!(submit(&mut repl, "(+ $_ #1) (get _)"), ["#43", "#43"]);
    assert_eq!(binding(&mut repl, "?"), result);
}
