use super::*;

#[test]
fn aliases_bind_the_callable_and_presets_once() {
    let mut repl = Repl::new();
    let evaluation = evaluate(
        &mut repl,
        r#"
        (let times #0)
        (let preset {value #42})
        (defn old-target (fixed arg) [(map/get $fixed value) $arg])
        (alias zz (old-target (do (set times (+ $times #1)) $preset)))
        (alias aa (zz))
        (set preset {value #0})
        (defn old-target (fixed arg) "new function")
        (alias zz (list #0))
        (aa "a b")
        (apply aa [#7])
        ((function aa) #8)
        (zz #9)
        (list $times (map/keys (alias)))
        (map/get (alias) aa)
        "#,
    );
    assert_eq!(
        values(&evaluation),
        [
            "(#42 \"a b\")",
            "(#42 #7)",
            "(#42 #8)",
            "(#0 #9)",
            "(#1 (\"aa\" \"zz\"))",
            "(zz)"
        ]
    );
    let remove = evaluate(&mut repl, "(unalias aa) (map/keys (alias))");
    assert_eq!(values(&remove), ["(\"zz\")"]);
}

#[test]
fn alias_names_and_targets_follow_command_namespace_rules() {
    let mut repl = Repl::new();
    evaluate(
        &mut repl,
        "(defn taken () #nil) (defstruct Point x) (alias mine (list)) (alias Future.x (list))",
    );
    for invalid in [
        "(alias dump (list))",
        "(alias taken (list))",
        "(alias Point.x (list))",
        "(alias a (Point #1))",
        "(alias a (if #true))",
        "(alias a (when #true))",
        "(defn mine () #nil)",
        "(defmacro mine () '#nil')",
        "(defstruct mine x)",
        "(defstruct Future x)",
        "(alias a ())",
        "(alias a list)",
        "(unalias unknown)",
    ] {
        assert!(
            evaluate(&mut repl, invalid)
                .iter()
                .any(|event| matches!(event, Event::Diagnostic(_))),
            "{invalid}",
        );
    }
    assert_eq!(
        values(&evaluate(&mut repl, "(map/keys (alias))")),
        ["(\"Future.x\" \"mine\")"]
    );
}

#[test]
fn aliases_complete_as_command_names_and_function_references() {
    let mut repl = Repl::new();
    evaluate(&mut repl, "(alias quick-list (list))");
    for (draft, completed) in [
        ("quick-l", "quick-list "),
        ("function quick-l", "function quick-list "),
    ] {
        repl.paste(draft);
        repl.input(Key::Tab);
        assert_eq!(repl.session.input.body, completed);
        repl.input(Key::ClearInput);
    }
    repl.reset();
    assert_eq!(values(&evaluate(&mut repl, "(map/keys (alias))")), ["()"]);
}
