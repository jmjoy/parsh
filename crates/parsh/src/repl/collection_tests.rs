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

#[test]
fn collections_match_constructors_and_preserve_types_and_comments() {
    let events = evaluate(
        &mut Repl::new(),
        r#"
        []{}
        (let value "two words")
        [text $value #1 #true #nil [] {}]
        {name parsh nested {items [one "two words"]} flag #true}
        (eq [one {key [two]}] (list one (map key (list two))))
        [; a (; $undefined [ignored {key value}]) b]
        {; text (; $undefined)}
        (str "$(eq [a] [a]) ${name} [x] {y}")
    "#,
    );
    assert!(diagnostics(&events).is_empty(), "{events:?}");
    assert_eq!(
        values(&events),
        [
            "()",
            "(map)",
            r#"("text" "two words" #1 #true #nil () (map))"#,
            r#"(map "name" "parsh" "nested" (map "items" ("one" "two words")) "flag" #true)"#,
            "#true",
            r#"(";" "a" "b")"#,
            r#"(map ";" "text")"#,
            r#""true \${name} [x] {y}""#,
        ]
    );
}

#[test]
fn collection_operands_evaluate_once_from_left_to_right() {
    let events = evaluate(
        &mut Repl::new(),
        r#"
        (let trace (str))
        [(do (set trace (str $trace a)) first)
         (do (set trace (str $trace b)) second)
         $trace]
        {(do (set trace (str $trace k)) key)
         (do (set trace (str $trace v)) [$trace])}
        (get trace)
    "#,
    );
    assert!(diagnostics(&events).is_empty(), "{events:?}");
    assert_eq!(
        values(&events),
        [
            r#"("first" "second" "ab")"#,
            r#"(map "key" ("abkv"))"#,
            r#""abkv""#,
        ]
    );
}

#[test]
fn collection_errors_preserve_source_positions_and_skip_invalid_values() {
    for (source, column, message) in [
        (
            "{orphan (set trace wrong) trailing}",
            1,
            "alternating keys and values",
        ),
        ("{key first key (set trace wrong)}", 12, "duplicate map key"),
        ("{(+ #1) (set trace wrong)}", 2, "map keys must be text"),
        ("{[a] (set trace wrong)}", 2, "map keys must be text"),
        ("[甲 $missing]", 4, "undefined variable $missing"),
    ] {
        let mut repl = Repl::new();
        evaluate(&mut repl, "(let trace saved)");
        let events = evaluate(&mut repl, source);
        let errors = diagnostics(&events);
        assert_eq!(errors.len(), 1, "{events:?}");
        assert!(
            errors[0].starts_with(&format!("interactive:1:{column}:")),
            "{errors:?}"
        );
        assert!(errors[0].contains(message), "{errors:?}");
        assert_eq!(values(&evaluate(&mut repl, "(get trace)")), [r#""saved""#]);
    }
}

#[test]
fn typed_and_pasted_collections_stay_inside_calls_and_keep_history() {
    for (source, accepted, expected) in [
        ("list [a b]", "(list [a b])", r#"(("a" "b"))"#),
        ("(list [a b])", "(list [a b])", r#"(("a" "b"))"#),
        (
            "list {key value}",
            "(list {key value})",
            r#"((map "key" "value"))"#,
        ),
    ] {
        for pasted in [false, true] {
            let mut repl = Repl::new();
            if pasted {
                repl.paste(source);
            } else {
                for ch in source.chars() {
                    repl.input(Key::Character(ch));
                }
            }
            assert_eq!(enter_value(&mut repl), expected);
            assert_eq!(*repl.session.history.entries(), [accepted]);
            repl.input(Key::Previous);
            assert_eq!(repl.session.input.source(), accepted);
            assert_eq!(repl.session.input.cursor, accepted.len() - 1);
            assert_eq!(enter_value(&mut repl), expected);
        }
    }
}

#[test]
fn collection_hints_and_incomplete_batches_follow_nested_delimiters() {
    for (source, close) in [
        ("list [a", ']'),
        ("list {key value", '}'),
        ("list [] {key [a", ']'),
    ] {
        let mut repl = Repl::new();
        repl.paste(source);
        let view = view_layout(&repl.session.input, None, "$ ", 80, 24);
        assert_eq!(view.dimmed_cells.last().unwrap().character, close);
        assert_eq!(repl.session.input.source(), format!("({source}"));
    }
    let mut repl = Repl::new();
    repl.paste("list [] {key [a");
    let events = repl.input(Key::Enter);
    assert!(diagnostics(&events).is_empty());
    assert_eq!(previous(&mut repl), Value::Nil);
    assert!(repl.session.history.entries().is_empty());
    assert_eq!(repl.session.input.source(), "(list [] {key [a\n");
    repl.paste("]})");
    assert_eq!(enter_value(&mut repl), r#"(() (map "key" ("a")))"#);

    repl.paste("(let skipped wrong) [a}");
    let events = repl.input(Key::Enter);
    assert!(diagnostics(&events)[0].contains("expected ']'"));
    assert_eq!(repl.session.input.body, "(let skipped wrong) [a}");
    repl.input(Key::ClearInput);
    assert!(diagnostics(&evaluate(&mut repl, "(get skipped)"))[0].contains("undefined variable"));
}

#[test]
fn yanking_explicit_calls_with_collections_preserves_source() {
    for (source, expected) in [
        ("(do [a])", r#"("a")"#),
        ("(do {key value})", r#"(map "key" "value")"#),
    ] {
        let mut repl = Repl::new();
        repl.paste(source);
        repl.input(Key::KillStart);
        repl.input(Key::Interrupt);
        repl.input(Key::Yank);
        assert_eq!(repl.session.input.source(), source);
        assert_eq!(enter_value(&mut repl), expected);
    }
}
