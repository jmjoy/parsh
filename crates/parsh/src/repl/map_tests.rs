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

#[test]
fn maps_preserve_value_types_text_keys_and_display_order() {
    assert_values(
        r#"
        (map)
        (let key dynamic)
        (let m (map $key #3 flag #true nothing #nil
                    items (list a b) nested (map x y) "" empty))
        (get m)
        (list (map/get $m dynamic) (map/get $m flag) (map/get $m nothing)
              (map/get $m items) (map/get $m nested) (map/get $m ""))
        (map 'a$b' 'x$y')
        "#,
        &[
            "(map)",
            r#"(map "dynamic" #3 "flag" #true "nothing" #nil "items" ("a" "b") "nested" (map "x" "y") "" "empty")"#,
            r#"(#3 #true #nil ("a" "b") (map "x" "y") "empty")"#,
            r#"(map "a\$b" "x\$y")"#,
        ],
    );
}

#[test]
fn updates_preserve_order_and_reinsertions_append() {
    assert_values(
        r#"
        (let m (map c third a first b second))
        (set m (map/set $m a updated))
        (set m (map/del $m c))
        (set m (map/set $m d fourth))
        (map/keys $m)
        (map/vals $m)
        (list (map/len $m) (map/has $m a) (map/has $m c))
        (set m (map/set $m c restored))
        (map/keys $m)
        (set m (map/del $m a))
        (set m (map/del $m b))
        (set m (map/del $m d))
        (set m (map/del $m c))
        (list (map/len $m) (map/keys $m) (map/vals $m) (get m))
        "#,
        &[
            r#"("a" "b" "d")"#,
            r#"("updated" "second" "fourth")"#,
            "(#3 #true #false)",
            r#"("a" "b" "d" "c")"#,
            "(#0 () () (map))",
        ],
    );
}

#[test]
fn map_commands_accept_values_and_updates_return_independent_maps() {
    assert_values(
        r#"
        (defn source () {first one second two})
        (map/get (source) first)
        (map/set {} key value)
        (map/del {first one second two} first)
        (map/has {first one} first)
        (map/keys (map/set (source) third three))
        (map/vals (map/del (source) first))
        (map/len (map/set (map/set {} first one) second two))
        (map/get (map/get {nested {key value}} nested) key)
        (let m (source))
        (let updated (map/set $m first changed))
        (let deleted (map/del $m first))
        (list $m $updated $deleted)
        (map/set $m first discarded)
        (map/del $m first)
        (map/get $m first)
        "#,
        &[
            r#""one""#,
            r#"(map "key" "value")"#,
            r#"(map "second" "two")"#,
            "#true",
            r#"("first" "second" "third")"#,
            r#"("two")"#,
            "#2",
            r#""value""#,
            r#"((map "first" "one" "second" "two") (map "first" "changed" "second" "two") (map "second" "two"))"#,
            r#"(map "first" "discarded" "second" "two")"#,
            r#"(map "second" "two")"#,
            r#""one""#,
        ],
    );
}

#[test]
fn map_equality_ignores_order_and_compares_nested_values_by_type() {
    assert_values(
        r#"
        (eq (map a (map x (list #1 #true)) b #nil)
            (map b #nil a (map x (list #1 #true))))
        (eq (list (map a one b two)) (list (map b two a one)))
        (eq (map a #1) (map a 1))
        (eq (map a #1) (map a #1.0))
        (eq (map a one) (map a two))
        (eq (map a one) (map b one))
        (eq (map) (list))
        "#,
        &[
            "#true", "#true", "#false", "#false", "#false", "#false", "#false",
        ],
    );
}

#[test]
fn defaults_are_eager_but_only_returned_for_missing_keys() {
    assert_values(
        r#"
        (let m (map present #nil))
        (let calls #0)
        (list (map/get $m present (set calls (+ $calls #100)))
              (map/get $m absent (do (set calls (+ $calls #1)) (list fallback)))
              $calls)
        (map/get $m added (do (set m (map/set $m added installed)) (map/get $m added)))
        (map/get $m added fallback)
        (list (map/get $m absent #nil))
        "#,
        &[
            r#"(#nil ("fallback") #101)"#,
            r#""installed""#,
            r#""installed""#,
            "(#nil)",
        ],
    );
}

#[test]
fn map_updates_follow_lexical_scope_and_copies_are_independent() {
    assert_values(
        r#"
        (let m (map k outer nested (map k original)))
        (let copy $m)
        (defn update () (set m (map/set $m k captured)))
        (defn caller (m) (update) (map/get $m k))
        (caller (map k argument))
        (list (map/get $m k) (map/get $copy k))
        (defn local (m) (set m (map/del (map/set $m k local) nested)) $m)
        (local $m)
        (let nested (map/get $m nested))
        (set nested (map/set $nested k changed))
        (map/get $m nested)
        (set m (map/set $m nested $nested))
        (map/get $copy nested)
        (let enumerated (map/vals $m))
        (defn replace (enumerated) (set enumerated (list replaced)))
        (replace $enumerated)
        (map/get $m nested)
        (set nested (map/set $nested k later))
        (set m (map/set $m nested $nested))
        (list $enumerated (map/get $m nested))
        "#,
        &[
            r#""argument""#,
            r#"("captured" "outer")"#,
            r#"(map "k" "local")"#,
            r#"(map "k" "original")"#,
            r#"(map "k" "original")"#,
            r#"(map "k" "changed")"#,
            r#"(("captured" (map "k" "changed")) (map "k" "later"))"#,
        ],
    );
}

#[test]
fn operands_evaluate_once_in_order_and_use_the_initial_map_value() {
    assert_values(
        r#"
        (let trace (str))
        (let m {start original})
        (let result
          (map/set (do (set trace (str $trace m)) $m)
                (do (set trace (str $trace k)) (set m (map/set $m from_key key-side)) target)
                (do (set trace (str $trace v)) (set m (map/set $m from_value value-side)) stored)))
        (list $trace $result $m)
        (set m {found original})
        (map/get $m (do (set m (map/set $m found changed)) found))
        (map/get $m found)
        (map/has $m (do (set m (map/set $m added value)) added))
        (map/has $m added)
        (map/del $m (do (set m scalar) found))
        (get m)
        (set m {start original})
        (map/set $m key (do (set m replacement) value))
        (get m)
        "#,
        &[
            r#"("mkv" (map "start" "original" "target" "stored") (map "start" "original" "from_key" "key-side" "from_value" "value-side"))"#,
            r#""original""#,
            r#""changed""#,
            "#false",
            "#true",
            r#"(map "added" "value")"#,
            r#""scalar""#,
            r#"(map "start" "original" "key" "value")"#,
            r#""replacement""#,
        ],
    );
}

#[test]
fn construction_errors_stop_before_invalid_values_and_locate_the_key() {
    let mut repl = Repl::new();
    execute(&mut repl, "(let trace (str))");
    let events = execute(
        &mut repl,
        r#"(map (do (set trace (str $trace k)) key)
               (do (set trace (str $trace v)) first)
               (do (set trace (str $trace d)) key)
               (set trace wrong))"#,
    );
    let errors = diagnostics(&events);
    assert_eq!(errors.len(), 1, "{events:?}");
    assert!(
        errors[0].contains("duplicate map key \"key\""),
        "{errors:?}"
    );
    assert!(errors[0].starts_with("interactive:3:16:"), "{errors:?}");
    assert_eq!(values(&execute(&mut repl, "(get trace)")), [r#""kvd""#]);

    let events = execute(&mut repl, "(map (set trace wrong))");
    assert!(diagnostics(&events)[0].contains("alternating keys and values"));
    assert_eq!(values(&execute(&mut repl, "(get trace)")), [r#""kvd""#]);

    for key in ["(+ #1)", "(eq a a)", "(do)", "(list a)", "(map)"] {
        let events = execute(&mut repl, &format!("(map {key} (set trace wrong))"));
        assert!(diagnostics(&events)[0].contains("map keys must be text"));
        assert!(diagnostics(&events)[0].starts_with("interactive:1:6:"));
        assert_eq!(values(&execute(&mut repl, "(get trace)")), [r#""kvd""#]);
    }
}

#[test]
fn commands_validate_map_values_and_arity_before_evaluating_later_arguments() {
    for (command, tail) in [
        ("map/get", "key"),
        ("map/set", "key value"),
        ("map/del", "key"),
        ("map/has", "key"),
        ("map/keys", ""),
        ("map/vals", ""),
        ("map/len", ""),
    ] {
        let mut repl = Repl::new();
        execute(
            &mut repl,
            "(let m (map key value)) (let text scalar) (let calls #0)",
        );
        for (target, message) in [
            ("m", "expected a map"),
            ("\"m\"", "expected a map"),
            ("bad-name", "expected a map"),
            ("missing", "expected a map"),
            ("$missing", "undefined variable $missing"),
            ("$text", "expected a map"),
            ("(+ #1)", "expected a map"),
            ("(eq a a)", "expected a map"),
            ("(do)", "expected a map"),
            ("[]", "expected a map"),
        ] {
            let source = format!("({command} {target} {tail})");
            let events = execute(&mut repl, &source);
            assert_eq!(diagnostics(&events).len(), 1, "{source}: {events:?}");
            assert!(
                diagnostics(&events)[0].contains(message),
                "{source}: {events:?}"
            );
            assert!(
                diagnostics(&events)[0]
                    .starts_with(&format!("interactive:1:{}:", command.len() + 3))
            );
        }
        for target in ["$m", "{key value}", "(map key value)", "(do $m)"] {
            let source = format!("({command} {target} {tail})");
            let events = execute(&mut repl, &source);
            assert!(diagnostics(&events).is_empty(), "{source}: {events:?}");
        }
        for source in [
            format!("({command})"),
            format!("({command} (set calls (+ $calls #1)) {tail} extra extra)"),
        ] {
            let events = execute(&mut repl, &source);
            assert!(
                diagnostics(&events)[0].contains("arguments"),
                "{source}: {events:?}"
            );
        }
        let effects = match command {
            "map/set" => " (set calls (+ $calls #1)) (set calls (+ $calls #1))",
            "map/get" | "map/del" | "map/has" => " (set calls (+ $calls #1))",
            _ => "",
        };
        let source = format!("({command} scalar{effects})");
        let events = execute(&mut repl, &source);
        assert!(
            diagnostics(&events)[0].contains("expected a map"),
            "{source}: {events:?}"
        );
        assert_eq!(
            values(&execute(&mut repl, "(get calls)")),
            [if command == "map/get" { "#1" } else { "#0" }]
        );
        let events = execute(
            &mut repl,
            &format!("(defn shadow (m) ({command} $m {tail})) (shadow text)"),
        );
        assert!(
            diagnostics(&events)[0].contains("expected a map"),
            "{events:?}"
        );
        assert_eq!(
            values(&execute(&mut repl, "(map/get $m key)")),
            [r#""value""#]
        );
    }
}

#[test]
fn missing_and_invalid_keys_report_errors_without_overwriting_or_evaluating_values() {
    let mut repl = Repl::new();
    execute(&mut repl, "(let m (map key value)) (let calls #0)");
    for command in ["map/get", "map/del"] {
        let source = format!("(set m ({command} $m missing))");
        let events = execute(&mut repl, &source);
        assert!(diagnostics(&events)[0].contains("map key \"missing\" not found"));
        assert!(diagnostics(&events)[0].starts_with(&format!(
            "interactive:1:{}:",
            source.find("missing").unwrap() + 1
        )));
    }
    for command in ["map/get", "map/set", "map/del", "map/has"] {
        for key in ["(+ #1)", "(eq a a)", "(do)", "(list)", "(map)"] {
            let tail = if command == "map/set" {
                " (set calls (+ $calls #1))"
            } else {
                ""
            };
            let events = execute(&mut repl, &format!("({command} $m {key}{tail})"));
            assert!(
                diagnostics(&events)[0].contains("map keys must be text"),
                "{events:?}"
            );
        }
    }
    assert_eq!(
        values(&execute(&mut repl, "(list $calls $m)")),
        [r#"(#0 (map "key" "value"))"#]
    );
    let events = execute(
        &mut repl,
        "(map/get $missing key (set calls (+ $calls #1)))",
    );
    assert!(diagnostics(&events)[0].contains("undefined variable $missing"));
    assert_eq!(values(&execute(&mut repl, "(get calls)")), ["#0"]);
}

#[test]
fn maps_are_rejected_in_scalar_positions() {
    for source in [
        "(str $m)",
        r#"(str "$m")"#,
        r#"(str "$(get m)")"#,
        "(export MAP $m)",
        "(printf %s $m)",
        "(printf %s (list $m))",
    ] {
        let mut repl = Repl::new();
        execute(&mut repl, "(let m (map key value))");
        let events = execute(&mut repl, source);
        assert!(
            diagnostics(&events)[0].contains("expected a scalar value, got a map"),
            "{events:?}"
        );
    }
}

#[test]
fn map_names_are_reserved_and_recognized_by_type() {
    for command in [
        "map", "map/get", "map/set", "map/del", "map/has", "map/keys", "map/vals", "map/len",
    ] {
        let mut repl = Repl::new();
        let events = execute(&mut repl, &format!("(str (type -t {command}))"));
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        assert_eq!(values(&events), [r#""builtin""#]);
        let events = execute(&mut repl, &format!("(defn {command} () (exit 9))"));
        assert!(diagnostics(&events)[0].contains("function name is reserved"));
    }
}
