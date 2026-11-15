use super::*;

fn diagnostics(events: &[Event]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Diagnostic(message) => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

fn outputs(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Output { bytes, .. } => Some(String::from_utf8(bytes.clone()).unwrap()),
            _ => None,
        })
        .collect()
}

fn assert_values(source: &str, expected: &[&str]) {
    let events = evaluate(&mut Repl::new(), source);
    assert!(diagnostics(&events).is_empty(), "{events:?}");
    assert_eq!(values(&events), expected, "{source}");
}

fn assert_diagnostic(source: &str, expected: &str) {
    let events = evaluate(&mut Repl::new(), source);
    assert!(
        diagnostics(&events)
            .iter()
            .any(|message| message.contains(expected)),
        "{source}: {events:?}"
    );
}

#[test]
fn structs_construct_access_and_print_in_field_order() {
    assert_values(
        r#"
        (defstruct Point x y)
        (Point 3 4)
        (let point (Point 3 4))
        (Point.x $point)
        (Point.y $point)
        (Point? $point)
        (Point? 7)
        "#,
        &[r#"(Point "3" "4")"#, r#""3""#, r#""4""#, "#true", "#false"],
    );
}

#[test]
fn dot_reads_fields_by_evaluated_text_names_without_a_type_name() {
    assert_values(
        r#"
        (defstruct Point x y)
        (defstruct Other x)
        (let point (Point #3 #4))
        (let field y)
        (list (. $point x) (. $point "y") (. $point $field)
              (. $point (str x)) (. (Other #5) x)
              (eq (. $point x) (Point.x $point)))
        "#,
        &["(#3 #4 #4 #3 #5 #true)"],
    );
}

#[test]
fn dot_preserves_field_types_and_composes_for_nested_structs() {
    assert_values(
        r#"
        (defstruct Point x y)
        (defstruct Segment start end)
        (let segment (Segment (Point #0 #1) (Point #3 #4)))
        (let first start)
        (let second x)
        (list (. $segment start x)
              (. $segment end x)
              (. $segment $first $second)
              (. $segment start)
              (eq (. $segment end) (Segment.end $segment)))
        (defstruct Box value)
        (let items [#7 #true #nil (bytes/from-text hello) [nested]
                    {key #1} $segment (function +) (command echo) (quote (a b))])
        (eq (list/map (fn (value) (. (Box $value) value)) $items) $items)
        "#,
        &["(#0 #3 #0 (Point #0 #1) #true)", "#true"],
    );
}

#[test]
fn dot_is_an_ordinary_function_and_does_not_replace_the_last_result() {
    assert_values(
        r#"
        (defstruct Point x y)
        (let point (Point #3 #4))
        (let getter (function .))
        (let name .)
        (defn answer () #42)
        (answer)
        (let saved $?)
        (list ($getter $point x) ((function .) $point y)
              (apply $getter [$point x]) (apply "." [$point y])
              ($name $point x) (. $? value) (eq $? $saved))
        "#,
        &["#42", "(#3 #4 #3 #4 #3 #42 #true)"],
    );
}

#[test]
fn dot_reads_runtime_records_and_preserves_bytes_and_nil() {
    assert_values(
        r#"
        (let result (capture/result + #2 #3))
        (list (. $result value) (bytes? (. $result stdout))
              (bytes? (. $result stderr)) (. $result termination) (. $result error)
              (eq (. $result stdout) (CallResult.stdout $result)))
        "#,
        &["(#5 #true #true #nil #nil #true)"],
    );
}

#[test]
fn dot_chains_through_nested_structs_and_reports_intermediate_errors() {
    assert_values(
        r#"
        (defstruct Point x)
        (defstruct Box point)
        (let box (Box (Point #7)))
        (list (. $box point x) (eq (. $box point) (Point #7)))
        "#,
        &["(#7 #true)"],
    );
    assert_diagnostic(
        "(defstruct Point x) (defstruct Box point) (let box (Box (Point #1))) (. $box point missing)",
        "Point has no field missing",
    );
    assert_diagnostic(
        "(defstruct Box point) (. (Box #1) point x)",
        ". expects a struct",
    );
}

#[test]
fn dot_checks_arguments_and_reports_errors_at_the_operand() {
    for (source, message, location) in [
        (
            "(.)",
            "expected at least 2 arguments, got 0",
            "interactive:1:1:",
        ),
        (
            "(. $undefined)",
            "expected at least 2 arguments, got 1",
            "interactive:1:1:",
        ),
        (
            "(. $undefined x extra)",
            "undefined variable $undefined",
            "interactive:1:4:",
        ),
        ("(. #1 x)", ". expects a struct", "interactive:1:4:"),
        ("(. {x #1} x)", ". expects a struct", "interactive:1:4:"),
        ("(. (Point #1) #1)", "expected text", "interactive:1:15:"),
        (
            "(. (Point #1) missing)",
            "Point has no field missing",
            "interactive:1:15:",
        ),
        (
            "(. (Point #1) \"\")",
            "Point has no field ",
            "interactive:1:15:",
        ),
    ] {
        let mut repl = Repl::new();
        evaluate(&mut repl, "(defstruct Point x)");
        let events = evaluate(&mut repl, source);
        let errors = diagnostics(&events);
        assert_eq!(errors.len(), 1, "{source}: {events:?}");
        assert!(errors[0].contains(message), "{source}: {errors:?}");
        assert!(errors[0].starts_with(location), "{source}: {errors:?}");
    }
    assert_diagnostic("(defn . () #nil)", "function name is reserved");
    assert_diagnostic("(defmacro . () (quote x))", "macro name is reserved");
}

#[test]
fn dot_evaluates_operands_once_in_order_and_stops_after_invalid_arguments() {
    let mut repl = Repl::new();
    evaluate(
        &mut repl,
        "(defstruct Point x) (defstruct Box point) (let trace (str))",
    );
    for callee in [".", "(function .)"] {
        evaluate(&mut repl, "(set trace (str))");
        let events = evaluate(
            &mut repl,
            &format!(
                "({callee} (do (set trace (str $trace v)) (Point #7))
                           (do (set trace (str $trace f)) x))"
            ),
        );
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        assert_eq!(values(&events), ["#7"]);
        assert_eq!(values(&evaluate(&mut repl, "(get trace)")), [r#""vf""#]);
        evaluate(&mut repl, "(set trace (str))");

        for (arguments, message, expected_trace) in [
            (
                "(set trace wrong)",
                "expected at least 2 arguments, got 1",
                r#""""#,
            ),
            (
                "(do (set trace v) #nil) (set trace wrong)",
                ". expects a struct",
                r#""v""#,
            ),
            (
                "(do (set trace v) (Point #7)) (do (set trace (str $trace f)) #nil)",
                "expected text",
                r#""vf""#,
            ),
        ] {
            let events = evaluate(&mut repl, &format!("({callee} {arguments})"));
            assert!(diagnostics(&events)[0].contains(message), "{events:?}");
            assert_eq!(
                values(&evaluate(&mut repl, "(get trace)")),
                [expected_trace]
            );
            evaluate(&mut repl, "(set trace (str))");
        }

        let events = evaluate(
            &mut repl,
            &format!(
                "({callee}
                    (do (set trace (str $trace v)) (Box (Point #7)))
                    (do (set trace (str $trace p)) point)
                    (do (set trace (str $trace f)) x))"
            ),
        );
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        assert_eq!(values(&events), ["#7"]);
        assert_eq!(values(&evaluate(&mut repl, "(get trace)")), [r#""vpf""#]);
    }
}

#[test]
fn structs_compare_by_name_and_fields_and_copy_with_bindings() {
    assert_values(
        r#"
        (defstruct Point x y)
        (eq (Point 1 2) (Point 1 2))
        (eq (Point 1 2) (Point 2 1))
        (let original (Point 1 2))
        (let copy $original)
        (set original (Point 9 9))
        (Point.x $copy)
        "#,
        &["#true", "#false", r#""1""#],
    );
}

#[test]
fn structs_nest_and_live_inside_collections() {
    assert_values(
        r#"
        (defstruct Point x y)
        (defstruct Segment start end)
        (let segment (Segment (Point 0 0) (Point 1 1)))
        (Segment.end $segment)
        (Point.x (Segment.start $segment))
        (list (Point 1 2))
        (map/get (map point (Point 1 2)) point)
        "#,
        &[
            r#"(Point "1" "1")"#,
            r#""0""#,
            r#"((Point "1" "2"))"#,
            r#"(Point "1" "2")"#,
        ],
    );
}

#[test]
fn redefining_a_struct_keeps_old_values_readable() {
    assert_values(
        r#"
        (defstruct Point x y)
        (let old (Point 1 2))
        (defstruct Point x)
        (Point 1)
        (Point.y $old)
        (. $old y)
        "#,
        &[r#"(Point "1")"#, r#""2""#, r#""2""#],
    );
}

#[test]
fn struct_definitions_are_validated() {
    assert_diagnostic(
        "(defstruct)",
        "defstruct expects a name and at least one field",
    );
    assert_diagnostic(
        "(defstruct Point)",
        "defstruct expects a name and at least one field",
    );
    assert_diagnostic(
        r#"(defstruct "Point" x)"#,
        "struct name must be a bare word",
    );
    assert_diagnostic("(defstruct Point.x x)", "struct name is reserved");
    assert_diagnostic("(defstruct Point x x)", "duplicate field x");
    assert_diagnostic("(defstruct Point x-y)", "invalid field name");
    assert_diagnostic(
        "(defstruct Point x) (defn Point () (str x))",
        "Point is already a struct",
    );
    assert_diagnostic(
        "(defn helper () (str x)) (defstruct helper x)",
        "helper is already a function",
    );
    assert_diagnostic(
        "(defmacro helper () `(str x)) (defstruct helper x)",
        "helper is already a macro",
    );
}

#[test]
fn struct_calls_are_validated() {
    assert_diagnostic(
        "(defstruct Point x) (Point 1 2)",
        "expected 1 arguments, got 2",
    );
    assert_diagnostic(
        "(defstruct Point x) (Point.x 5)",
        "Point.x expects a Point value",
    );
    assert_diagnostic(
        "(defstruct Point x) (defstruct Other y) (Point.x (Other 1))",
        "Point.x expects a Point value",
    );
    assert_diagnostic(
        "(defstruct Point x) (Point.z (Point 1))",
        "Point has no field z",
    );
    assert_diagnostic(
        "(defstruct Point x) (str (Point 1))",
        "expected a scalar value, got a struct",
    );
    assert_diagnostic(
        "(defstruct Point x) (for value (Point 1) (str $value))",
        "for expects a list",
    );
}

#[test]
fn type_describes_struct_members_and_macros() {
    let events = evaluate(
        &mut Repl::new(),
        r#"
        (defstruct Point x y)
        (defmacro unless (condition body) `(if (not ,$condition) ,$body))
        (type Point)
        (type Point.x)
        (type Point?)
        (type unless)
        (type .)
        (type -t .)
        (type -t Point Point.x Point? unless)
        (type -f Point)
        "#,
    );
    assert_eq!(
        outputs(&events),
        "Point is a struct\n\
         Point.x is a struct accessor\n\
         Point? is a struct predicate\n\
         unless is a macro\n\
         . is a shell builtin\n\
         builtin\n\
         struct\nstruct\nstruct\nmacro\n\
         type: Point: not found\n"
    );
}

#[test]
fn completion_offers_struct_members_and_macros() {
    let mut repl = Repl::new();
    let events = evaluate(
        &mut repl,
        "(defstruct Point x y) (defmacro unless (condition body) `(str x))",
    );
    assert!(diagnostics(&events).is_empty(), "{events:?}");
    let mut complete = |prefix: &str| -> Vec<String> {
        repl.shell
            .completion_commands(prefix)
            .into_iter()
            .map(|candidate| candidate.text)
            .collect()
    };
    assert_eq!(complete("Point."), ["Point.x", "Point.y"]);
    assert_eq!(complete("Point?"), ["Point?"]);
    assert_eq!(complete("unl"), ["unless"]);
    assert!(complete(".").iter().any(|name| name == "."));
}
