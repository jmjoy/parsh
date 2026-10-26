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
    let events = execute(&mut Repl::new(), source);
    assert!(diagnostics(&events).is_empty(), "{events:?}");
    assert_eq!(values(&events), expected, "{source}");
}

fn assert_diagnostic(source: &str, expected: &str) {
    let events = execute(&mut Repl::new(), source);
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
        (mget (map point (Point 1 2)) point)
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
        "#,
        &[r#"(Point "1")"#, r#""2""#],
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
    let events = execute(
        &mut Repl::new(),
        r#"
        (defstruct Point x y)
        (defmacro unless (condition body) `(if (not ,$condition) ,$body))
        (type Point)
        (type Point.x)
        (type Point?)
        (type unless)
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
         struct\nstruct\nstruct\nmacro\n\
         type: Point: not found\n"
    );
}

#[test]
fn completion_offers_struct_members_and_macros() {
    let mut repl = Repl::new();
    let events = execute(
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
}
