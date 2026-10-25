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
fn quoted_forms_print_as_source_and_compare_by_syntax() {
    assert_values(
        r#"
        (quote (a b))
        (eq (quote (a b)) (quote (a   b)))
        (eq (quote (a b)) (quote (a c)))
        (quote "text $x")
        "#,
        &[r#"(a b)"#, "true", "false", r#""text $x""#],
    );
}

#[test]
fn quasiquote_substitutes_unquoted_values() {
    assert_values(
        r#"
        (let name world)
        (list `(str "hello " ,$name))
        (quasiquote (list 1 ,(+ 1 2)))
        (list `(a ,(+ 1 2)))
        "#,
        &[r#"((str "hello " "world"))"#, "(list 1 3)", "((a 3))"],
    );
}

#[test]
fn macros_expand_then_evaluate_in_the_caller() {
    assert_values(
        r#"
        (defmacro unless (condition body)
          `(if (not ,$condition) ,$body))
        (unless (eq 1 2) (str taken))
        (unless (eq 1 1) (str skipped))
        "#,
        &[r#""taken""#],
    );
}

#[test]
fn macros_can_run_inside_conditions() {
    assert_values(
        r#"
        (defmacro always () `(eq 1 1))
        (if (always) (str yes))
        "#,
        &[r#""yes""#],
    );
}

#[test]
fn unquote_splicing_flattens_list_forms_and_form_lists() {
    assert_values(
        r#"
        (defmacro begin (body)
          `(do ,@$body))
        (begin ((str a) (str b)))
        (let parts (quote ((str x) (str y))))
        `(do ,@$parts)
        "#,
        &[r#""b""#, "(do (str x) (str y))"],
    );
}

#[test]
fn macro_bodies_read_their_defining_scope() {
    assert_values(
        r#"
        (let suffix "-ish")
        (defmacro shout (text) `(str ,$text ,$suffix))
        (shout word)
        "#,
        &[r#""word-ish""#],
    );
}

#[test]
fn macros_can_expand_into_other_macro_calls() {
    assert_values(
        r#"
        (defmacro twice (body) `(do ,$body ,$body))
        (defmacro four (body) `(twice (twice ,$body)))
        (four (str tick))
        "#,
        &[r#""tick""#],
    );
}

#[test]
fn macros_reject_wrong_arity_and_non_forms() {
    assert_diagnostic(
        "(defmacro one (value) value) (one)",
        "expected 1 arguments, got 0",
    );
    assert_diagnostic(
        "(defmacro bad () 1) (bad)",
        "macro bad must return a quoted form",
    );
    assert_diagnostic(
        "(defmacro stringly (value) (str $value)) (stringly text)",
        "expected a scalar value, got a form",
    );
}

#[test]
fn unquote_outside_a_template_is_an_error() {
    assert_diagnostic(
        "(unquote (str x))",
        "unquote is only valid inside a quasiquote",
    );
    assert_diagnostic(
        "(unquote-splicing (list x))",
        "unquote-splicing is only valid inside a quasiquote",
    );
    assert_diagnostic(
        "(quasiquote (do ,@(str x)))",
        "unquote-splicing expects a list form or a list of quoted forms",
    );
    assert_diagnostic(
        "(quasiquote (do ,(list 1 2)))",
        "unquote expects a quoted form or a scalar value",
    );
    assert_diagnostic("(quasiquote (do ,@))", "expected an expression after ','");
}

#[test]
fn recursive_macro_expansion_stops_at_the_limit() {
    assert_diagnostic(
        "(defmacro again () `(again)) (again)",
        "macro expansion exceeds 32 levels",
    );
}

#[test]
fn macro_names_cannot_collide_with_other_definitions() {
    assert_diagnostic("(defmacro str (value) value)", "macro name is reserved");
    assert_diagnostic(
        "(defn occupied () (str x)) (defmacro occupied (value) value)",
        "occupied is already a function",
    );
    assert_diagnostic(
        "(defstruct Point x) (defmacro Point (value) value)",
        "Point is already a struct",
    );
    assert_diagnostic(
        "(defstruct Point x) (defmacro Point.x (value) value)",
        "Point.x is a struct member",
    );
    assert_diagnostic(
        "(defmacro occupied (value) value) (defn occupied () (str x))",
        "occupied is already a macro",
    );
    assert_values(
        "(defmacro redefined () `(str first)) (defmacro redefined () `(str second)) (redefined)",
        &[r#""second""#],
    );
}

#[test]
fn nested_quasiquotes_are_copied_literally() {
    assert_values(
        r#"
        (let value inserted)
        (quasiquote (outer (quasiquote (inner ,$value))))
        "#,
        &[r#"(outer (quasiquote (inner (unquote $value))))"#],
    );
}
