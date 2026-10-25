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

#[test]
fn collections_match_constructors_and_preserve_types_and_comments() {
    let events = execute(
        &mut Repl::new(),
        r#"
        []{}
        (let value "two words")
        [text $value (+ 1) (eq a a) (do) [] {}]
        {name parsh nested {items [one "two words"]} flag (eq a a)}
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
            r#"("text" "two words" 1 true nil () (map))"#,
            r#"(map "name" "parsh" "nested" (map "items" ("one" "two words")) "flag" true)"#,
            "true",
            r#"(";" "a" "b")"#,
            r#"(map ";" "text")"#,
            r#""true \${name} [x] {y}""#,
        ]
    );
}

#[test]
fn collection_operands_evaluate_once_from_left_to_right() {
    let events = execute(
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
        ("{(+ 1) (set trace wrong)}", 2, "map keys must be text"),
        ("{[a] (set trace wrong)}", 2, "map keys must be text"),
        ("[甲 $missing]", 4, "undefined variable $missing"),
    ] {
        let mut repl = Repl::new();
        execute(&mut repl, "(let trace saved)");
        let events = execute(&mut repl, source);
        let errors = diagnostics(&events);
        assert_eq!(errors.len(), 1, "{events:?}");
        assert!(
            errors[0].starts_with(&format!("interactive:1:{column}:")),
            "{errors:?}"
        );
        assert!(errors[0].contains(message), "{errors:?}");
        assert_eq!(values(&execute(&mut repl, "(get trace)")), [r#""saved""#]);
    }
}

#[test]
fn typed_and_pasted_collections_submit_with_the_matching_closer_and_keep_history() {
    for (source, accepted, expected) in [
        ("[a b", "[a b]", r#"("a" "b")"#),
        ("[a b]", "[a b]", r#"("a" "b")"#),
        ("{key value", "{key value}", r#"(map "key" "value")"#),
        ("{key value}", "{key value}", r#"(map "key" "value")"#),
        ("list [a b]", "(list [a b])", r#"(("a" "b"))"#),
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
            let events = repl.input(Key::Enter);
            assert!(diagnostics(&events).is_empty(), "{events:?}");
            assert_eq!(values(&events), [expected], "{source}");
            assert_eq!(repl.session.history.entries, [accepted]);
            repl.input(Key::Previous);
            assert_eq!(repl.session.input.source(), accepted);
            assert_eq!(repl.session.input.cursor, accepted.len() - 1);
            assert_eq!(values(&repl.input(Key::Enter)), [expected]);
        }
    }
}

#[test]
fn collection_hints_and_incomplete_batches_follow_nested_delimiters() {
    for (source, close) in [("[a", ']'), ("{key value", '}'), ("[] {key [a", ']')] {
        let mut repl = Repl::new();
        repl.paste(source);
        let view = view_layout(&repl.session.input, None, "$ ", 80, 24);
        assert_eq!(view.dimmed_cells.last().unwrap().character, close);
        assert_eq!(repl.session.input.source(), source);
    }
    let mut repl = Repl::new();
    repl.paste("[] {key [a");
    let events = repl.input(Key::Enter);
    assert!(values(&events).is_empty());
    assert!(diagnostics(&events).is_empty());
    assert!(repl.session.history.entries.is_empty());
    assert_eq!(repl.session.input.source(), "[] {key [a\n");
    repl.paste("]}");
    assert_eq!(
        values(&repl.input(Key::Enter)),
        ["()", r#"(map "key" ("a"))"#]
    );

    repl.paste("(let skipped wrong) [a}");
    let events = repl.input(Key::Enter);
    assert!(diagnostics(&events)[0].contains("expected ']'"));
    assert_eq!(repl.session.input.body, "(let skipped wrong) [a}");
    repl.input(Key::ClearInput);
    assert!(diagnostics(&execute(&mut repl, "(get skipped)"))[0].contains("undefined variable"));
}

#[test]
fn yanking_collections_into_a_fresh_prompt_preserves_explicit_source() {
    for (source, expected) in [
        ("[a]", r#"("a")"#),
        ("{key value}", r#"(map "key" "value")"#),
    ] {
        let mut repl = Repl::new();
        repl.paste(source);
        repl.input(Key::KillStart);
        repl.input(Key::Interrupt);
        let events = repl.input(Key::Yank);
        assert!(values(&events).is_empty());
        assert_eq!(repl.session.input.source(), source);
        assert_eq!(values(&repl.input(Key::Enter)), [expected]);
    }
}
