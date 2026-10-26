use super::*;
use crate::{number::Number, reader::complete};

fn values(events: &[Event]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Value(value) => Some(value.as_str()),
            _ => None,
        })
        .collect()
}

fn succeeds(source: &str, expected: &[&str]) {
    let events = execute(&mut Repl::new(), source);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(_))),
        "{events:?}"
    );
    assert_eq!(values(&events), expected, "{source}");
}

fn fails(repl: &mut Repl, source: &str, expected: &str) {
    let events = execute(repl, source);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(message) if message.contains(expected))),
        "{source}: {events:?}"
    );
    repl.input(Key::ClearInput);
}

#[test]
fn reader_preserves_literal_types_and_normalizes_source() {
    for (source, kind, printed) in [
        ("#001", Kind::Number(Number::Int(1)), "#1"),
        ("#+1", Kind::Number(Number::Int(1)), "#1"),
        (
            "#-9223372036854775808",
            Kind::Number(Number::Int(i64::MIN)),
            "#-9223372036854775808",
        ),
        (
            "#9223372036854775807",
            Kind::Number(Number::Int(i64::MAX)),
            "#9223372036854775807",
        ),
        ("#.5", Kind::Number(Number::Float(0.5)), "#0.5"),
        ("#1.", Kind::Number(Number::Float(1.0)), "#1.0"),
        ("#1e2", Kind::Number(Number::Float(100.0)), "#100.0"),
        ("#true", Kind::Bool(true), "#true"),
        ("#false", Kind::Bool(false), "#false"),
        ("#nil", Kind::Nil, "#nil"),
    ] {
        let Read::Form(expr) = Reader::new(source).read() else {
            panic!("{source}")
        };
        assert_eq!(expr.kind, kind, "{source}");
        assert_eq!(expr.source(), printed);
        assert!(complete(source));
        let Read::Form(round_trip) = Reader::new(&expr.source()).read() else {
            panic!("{source}")
        };
        assert_eq!(round_trip.kind, kind);
    }
    let Read::Form(Expr {
        kind: Kind::Number(Number::Float(value)),
        ..
    }) = Reader::new("#-0.0").read()
    else {
        panic!()
    };
    assert_eq!(value.to_bits(), (-0.0_f64).to_bits());
    for source in [
        "#",
        "#flase",
        "#False",
        "#t",
        "#fff",
        "#123abc",
        "#1e",
        "#1e309",
        "#9223372036854775808",
        "#0xff",
        "#$x",
        "#(str 1)",
    ] {
        assert!(
            matches!(
                Reader::new(source).read(),
                Read::Error(Diagnostic { offset: 0, .. })
            ),
            "{source}"
        );
        assert!(!complete(source));
    }
}

#[test]
fn comments_shebangs_and_text_do_not_parse_literals() {
    let mut reader =
        Reader::new("#!/usr/bin/env parsh\n(; #fff (; #flase) [#bad {#bad #}] \"#bad\") #2");
    assert!(matches!(
        reader.read(),
        Read::Form(Expr {
            kind: Kind::Comment,
            ..
        })
    ));
    assert!(matches!(
        reader.read(),
        Read::Form(Expr {
            kind: Kind::Number(Number::Int(2)),
            ..
        })
    ));
    for source in ["(; #bad [)", "(; #bad 'unterminated)"] {
        assert!(matches!(
            Reader::new(source).read(),
            Read::Error(_) | Read::Incomplete(_)
        ));
    }
    succeeds(
        r##"[1 true nil abc#1 "#1" '#false']"##,
        &[r##"("1" "true" "nil" "abc#1" "#1" "#false")"##],
    );
    succeeds(
        r##"(let x "#1") (list (typeof $x) (typeof "#false") (str #1 #1.0 #false #nil))"##,
        &[r#"("text" "text" "11.0false")"#],
    );
}

#[test]
fn literals_work_in_typed_collections_and_conditions() {
    succeeds(
        r#"
        (list (typeof #1) (typeof #1.0) (typeof #false) (typeof #nil))
        [#1 #1.0 #true #false #nil {enabled #false}]
        [(eq #1 1) (eq #1 #1.0) (= #1 #1.0) (eq false #false)]
        (if #false (unavailable) (+ #1 #2))
        #nil
    "#,
        &[
            r#"("int" "float" "bool" "nil")"#,
            "(#1 #1.0 #true #false #nil (map \"enabled\" #false))",
            "(#false #false #true #false)",
            "#3",
        ],
    );
    fails(
        &mut Repl::new(),
        "(if #1 (str bad))",
        "a condition must be a boolean",
    );
    fails(&mut Repl::new(), "{#1 ignored}", "map keys must be text");
}

#[test]
fn conversions_agree_in_direct_referenced_and_applied_calls() {
    for (name, argument, expected) in [
        ("int", "\" -001 \"", "#-1"),
        ("int", "#1.0", "#1"),
        ("int", "\"1e2\"", "#100"),
        ("int", "#-0.0", "#0"),
        ("int", "#9223372036854775807", "#9223372036854775807"),
        ("int", "#-9223372036854775808.0", "#-9223372036854775808"),
        ("float", "#1", "#1.0"),
        ("float", "\" .5 \"", "#0.5"),
        ("float", "#-0.0", "#-0.0"),
        ("float", "#9007199254740993", "#9007199254740992.0"),
        ("bool", "#true", "#true"),
        ("bool", "\" false \"", "#false"),
    ] {
        succeeds(
            &format!(
                "({name} {argument}) ((function {name}) {argument}) (apply (function {name}) [{argument}])"
            ),
            &[expected, expected, expected],
        );
    }
    succeeds(
        "(= (float \"9223372036854775808\") #9223372036854775808.0)",
        &["#true"],
    );
}

#[test]
fn invalid_conversions_fail_before_continuing_and_evaluate_once() {
    for (source, message) in [
        ("(int #1.5)", "expected an integer value"),
        ("(int \"1.5\")", "expected an integer value"),
        ("(int #9223372036854775808.0)", "out of range"),
        ("(int \"9223372036854775808\")", "out of range"),
        ("(float \"1e309\")", "number must be finite"),
        ("(float \"NaN\")", "number must be finite"),
        ("(int #true)", "expected a number"),
        ("(float #false)", "expected a number"),
        ("(bool 0)", "expected a boolean"),
        ("(bool #1)", "expected a boolean"),
        ("(bool \"TRUE\")", "expected a boolean"),
        ("(bool \"#false\")", "expected a boolean"),
        ("(int \"#1\")", "invalid number"),
        ("(int #nil)", "expected a number"),
        ("(float [])", "expected a number"),
        ("(bool {})", "expected a boolean"),
    ] {
        fails(&mut Repl::new(), source, message);
    }
    let mut repl = Repl::new();
    execute(&mut repl, "(let calls #0)");
    for name in ["int", "float", "bool"] {
        fails(
            &mut repl,
            &format!("({name} (set calls #99) (set calls #99))"),
            "expected 1 arguments",
        );
        assert_eq!(values(&execute(&mut repl, "(get calls)")), ["#0"]);
    }
    fails(
        &mut repl,
        "(int (do (set calls (+ $calls #1)) #1.5))",
        "integer value",
    );
    assert_eq!(values(&execute(&mut repl, "(get calls)")), ["#1"]);
}

#[test]
fn macro_unquotes_preserve_scalar_types_and_form_equality() {
    succeeds(
        r#"
        (defmacro keep (value) `(list ,$value ,(+ #1 #2) ,(eq a a) ,(do)))
        (keep #1.0)
        (quote #001)
        (eq (quote #001) (quote #1))
        (eq (quote #1) (quote #1.0))
        (quasiquote (list ,#false ,#nil ,#-0.0))
    "#,
        &[
            "(#1.0 #3 #true #nil)",
            "#1",
            "#true",
            "#false",
            "(list #false #nil #-0.0)",
        ],
    );
}

#[test]
fn non_text_command_heads_do_not_evaluate_arguments() {
    let mut repl = Repl::new();
    execute(
        &mut repl,
        "(let trace #0) (let n #1) (let b #false) (let empty #nil)",
    );
    for head in ["#1", "#false", "#nil", "$n", "$b", "$empty", "(+ #1)"] {
        fails(
            &mut repl,
            &format!("({head} (set trace #99))"),
            "is not callable",
        );
        assert_eq!(values(&execute(&mut repl, "(get trace)")), ["#0"]);
    }
    succeeds(
        "(let command str) ($command #1) ((fn () #1))",
        &[r#""1""#, "#1"],
    );
}

#[test]
fn literal_input_hides_hints_and_preserves_history_and_editing() {
    for pasted in [false, true] {
        let mut repl = Repl::new();
        let source = " #1.0";
        if pasted {
            repl.paste(source);
        } else {
            for ch in source.chars() {
                repl.input(Key::Character(ch));
            }
        }
        let view = view_layout(&repl.session.input, None, "$ ", 80, 24);
        assert_eq!(view.lines, ["$  #1.0"]);
        assert!(view.dimmed_cells.is_empty());
        assert_eq!(view.cursor, (0, source.len() + 2));
        assert_eq!(values(&repl.input(Key::Enter)), ["#1.0"]);
        assert_eq!(repl.session.history.entries, [source]);
        repl.input(Key::Previous);
        assert_eq!(repl.session.input.cursor, source.len());
        assert_eq!(values(&repl.input(Key::Enter)), ["#1.0"]);
    }
    let mut repl = Repl::new();
    repl.input(Key::Character('#'));
    assert!(
        view_layout(&repl.session.input, None, "$ ", 80, 24)
            .dimmed_cells
            .is_empty()
    );
    repl.input(Key::Backspace);
    assert_eq!(
        view_layout(&repl.session.input, None, "$ ", 80, 24)
            .dimmed_cells
            .len(),
        2
    );
    repl.paste("#1");
    repl.input(Key::Undo);
    assert!(repl.session.input.body.is_empty());
    repl.paste("#1");
    repl.input(Key::Home);
    repl.input(Key::KillEnd);
    repl.input(Key::Yank);
    assert_eq!(repl.session.input.source(), "#1");
    assert_eq!(values(&repl.input(Key::Enter)), ["#1"]);
    repl.paste("#f");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.source(), "#false ");
    assert_eq!(values(&repl.input(Key::Enter)), ["#false"]);
    assert!(values(&execute(&mut repl, "#nil")).is_empty());
    succeeds("#1 (str next) #false", &["#1", r#""next""#, "#false"]);
}
