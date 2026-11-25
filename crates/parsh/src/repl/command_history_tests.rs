use super::*;

#[test]
fn history_queries_the_same_entries_as_recall_after_each_form_finishes() {
    let mut repl = Repl::new();
    execute(&mut repl, "(str Git)");
    execute(&mut repl, "(str git-old)");
    execute(&mut repl, "(str other)");
    execute(&mut repl, "(str git-new)");
    execute(&mut repl, "(str git-new)");
    execute(&mut repl, "(; git comment)");
    let failing = "(throw git-error)";
    execute(&mut repl, &format!("{failing} (str git-skipped)"));

    assert_eq!(
        values(&evaluate(&mut repl, r#"(history "git" #2) (history #0)"#)),
        [r#"("(throw git-error)" "(str git-new)")"#, "()"]
    );
    let query = r#"(history "query-self")"#;
    execute(&mut repl, query);
    assert_eq!(previous(&mut repl), Value::List(vec![]));
    repl.input(Key::Previous);
    assert_eq!(repl.session.input.body, query);
    assert_eq!(repl.session.history.entries().len(), 6);
    repl.input(Key::ClearInput);
    assert!(matches!(
        execute(&mut repl, "(exit 7)").last(),
        Some(Event::Exit(7))
    ));
    assert_eq!(repl.session.history.entries().back().unwrap(), "(exit 7)");
}

#[test]
fn history_rejects_invalid_queries_and_reset_clears_memory() {
    let mut repl = Repl::new();
    for source in [
        "(history #-1)",
        "(history #1.0)",
        "(history #true)",
        "(history query #nil)",
        "(history #1 query)",
    ] {
        assert!(
            evaluate(&mut repl, source)
                .iter()
                .any(|event| matches!(event, Event::Diagnostic(_))),
            "{source}",
        );
    }
    execute(&mut repl, "(str kept)");
    assert_eq!(
        values(&evaluate(&mut repl, "(history)")),
        [r#"("(str kept)")"#]
    );
    repl.reset();
    assert_eq!(values(&evaluate(&mut repl, "(history)")), ["()"]);
}
