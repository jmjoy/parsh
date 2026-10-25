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
        (let m (map $key (+ 3) flag (eq a a) nothing (do)
                    items (list a b) nested (map x y) "" empty))
        (get m)
        (list (mget $m dynamic) (mget $m flag) (mget $m nothing)
              (mget $m items) (mget $m nested) (mget $m ""))
        (map 'a$b' 'x$y')
        "#,
        &[
            "(map)",
            r#"(map "dynamic" 3 "flag" true "nothing" nil "items" ("a" "b") "nested" (map "x" "y") "" "empty")"#,
            r#"(3 true nil ("a" "b") (map "x" "y") "empty")"#,
            r#"(map "a\$b" "x\$y")"#,
        ],
    );
}

#[test]
fn updates_preserve_order_and_reinsertions_append() {
    assert_values(
        r#"
        (let m (map c third a first b second))
        (set m (mset $m a updated))
        (set m (mdel $m c))
        (set m (mset $m d fourth))
        (mkeys $m)
        (mvals $m)
        (list (mlen $m) (mhas $m a) (mhas $m c))
        (set m (mset $m c restored))
        (mkeys $m)
        (set m (mdel $m a))
        (set m (mdel $m b))
        (set m (mdel $m d))
        (set m (mdel $m c))
        (list (mlen $m) (mkeys $m) (mvals $m) (get m))
        "#,
        &[
            r#"("a" "b" "d")"#,
            r#"("updated" "second" "fourth")"#,
            "(3 true false)",
            r#"("a" "b" "d" "c")"#,
            "(0 () () (map))",
        ],
    );
}

#[test]
fn map_commands_accept_values_and_updates_return_independent_maps() {
    assert_values(
        r#"
        (defn source () {first one second two})
        (mget (source) first)
        (mset {} key value)
        (mdel {first one second two} first)
        (mhas {first one} first)
        (mkeys (mset (source) third three))
        (mvals (mdel (source) first))
        (mlen (mset (mset {} first one) second two))
        (mget (mget {nested {key value}} nested) key)
        (let m (source))
        (let updated (mset $m first changed))
        (let deleted (mdel $m first))
        (list $m $updated $deleted)
        (mset $m first discarded)
        (mdel $m first)
        (mget $m first)
        "#,
        &[
            r#""one""#,
            r#"(map "key" "value")"#,
            r#"(map "second" "two")"#,
            "true",
            r#"("first" "second" "third")"#,
            r#"("two")"#,
            "2",
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
        (eq (map a (map x (list (+ 1) (eq x x))) b (do))
            (map b (do) a (map x (list (+ 1) (eq x x)))))
        (eq (list (map a one b two)) (list (map b two a one)))
        (eq (map a (+ 1)) (map a 1))
        (eq (map a (+ 1)) (map a (+ 1.0)))
        (eq (map a one) (map a two))
        (eq (map a one) (map b one))
        (eq (map) (list))
        "#,
        &["true", "true", "false", "false", "false", "false", "false"],
    );
}

#[test]
fn defaults_are_eager_but_only_returned_for_missing_keys() {
    assert_values(
        r#"
        (let m (map present (do)))
        (let calls (+ 0))
        (list (mget $m present (set calls (+ $calls 100)))
              (mget $m absent (do (set calls (+ $calls 1)) (list fallback)))
              $calls)
        (mget $m added (do (set m (mset $m added installed)) (mget $m added)))
        (mget $m added fallback)
        (list (mget $m absent (do)))
        "#,
        &[
            r#"(nil ("fallback") 101)"#,
            r#""installed""#,
            r#""installed""#,
            "(nil)",
        ],
    );
}

#[test]
fn map_updates_follow_lexical_scope_and_copies_are_independent() {
    assert_values(
        r#"
        (let m (map k outer nested (map k original)))
        (let copy $m)
        (defn update () (set m (mset $m k captured)))
        (defn caller (m) (update) (mget $m k))
        (caller (map k argument))
        (list (mget $m k) (mget $copy k))
        (defn local (m) (set m (mdel (mset $m k local) nested)) $m)
        (local $m)
        (let nested (mget $m nested))
        (set nested (mset $nested k changed))
        (mget $m nested)
        (set m (mset $m nested $nested))
        (mget $copy nested)
        (let enumerated (mvals $m))
        (defn replace (enumerated) (set enumerated (list replaced)))
        (replace $enumerated)
        (mget $m nested)
        (set nested (mset $nested k later))
        (set m (mset $m nested $nested))
        (list $enumerated (mget $m nested))
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
          (mset (do (set trace (str $trace m)) $m)
                (do (set trace (str $trace k)) (set m (mset $m from_key key-side)) target)
                (do (set trace (str $trace v)) (set m (mset $m from_value value-side)) stored)))
        (list $trace $result $m)
        (set m {found original})
        (mget $m (do (set m (mset $m found changed)) found))
        (mget $m found)
        (mhas $m (do (set m (mset $m added value)) added))
        (mhas $m added)
        (mdel $m (do (set m scalar) found))
        (get m)
        (set m {start original})
        (mset $m key (do (set m replacement) value))
        (get m)
        "#,
        &[
            r#"("mkv" (map "start" "original" "target" "stored") (map "start" "original" "from_key" "key-side" "from_value" "value-side"))"#,
            r#""original""#,
            r#""changed""#,
            "false",
            "true",
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

    for key in ["(+ 1)", "(eq a a)", "(do)", "(list a)", "(map)"] {
        let events = execute(&mut repl, &format!("(map {key} (set trace wrong))"));
        assert!(diagnostics(&events)[0].contains("map keys must be text"));
        assert!(diagnostics(&events)[0].starts_with("interactive:1:6:"));
        assert_eq!(values(&execute(&mut repl, "(get trace)")), [r#""kvd""#]);
    }
}

#[test]
fn commands_validate_map_values_and_arity_before_evaluating_later_arguments() {
    for (command, tail) in [
        ("mget", "key"),
        ("mset", "key value"),
        ("mdel", "key"),
        ("mhas", "key"),
        ("mkeys", ""),
        ("mvals", ""),
        ("mlen", ""),
    ] {
        let mut repl = Repl::new();
        execute(
            &mut repl,
            "(let m (map key value)) (let text scalar) (let calls (+ 0))",
        );
        for (target, message) in [
            ("m", "expected a map"),
            ("\"m\"", "expected a map"),
            ("bad-name", "expected a map"),
            ("missing", "expected a map"),
            ("$missing", "undefined variable $missing"),
            ("$text", "expected a map"),
            ("(+ 1)", "expected a map"),
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
            format!("({command} (set calls (+ $calls 1)) {tail} extra extra)"),
        ] {
            let events = execute(&mut repl, &source);
            assert!(
                diagnostics(&events)[0].contains("arguments"),
                "{source}: {events:?}"
            );
        }
        let effects = match command {
            "mset" => " (set calls (+ $calls 1)) (set calls (+ $calls 1))",
            "mget" | "mdel" | "mhas" => " (set calls (+ $calls 1))",
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
            [if command == "mget" { "1" } else { "0" }]
        );
        let events = execute(
            &mut repl,
            &format!("(defn shadow (m) ({command} $m {tail})) (shadow text)"),
        );
        assert!(
            diagnostics(&events)[0].contains("expected a map"),
            "{events:?}"
        );
        assert_eq!(values(&execute(&mut repl, "(mget $m key)")), [r#""value""#]);
    }
}

#[test]
fn missing_and_invalid_keys_report_errors_without_overwriting_or_evaluating_values() {
    let mut repl = Repl::new();
    execute(&mut repl, "(let m (map key value)) (let calls (+ 0))");
    for command in ["mget", "mdel"] {
        let source = format!("(set m ({command} $m missing))");
        let events = execute(&mut repl, &source);
        assert!(diagnostics(&events)[0].contains("map key \"missing\" not found"));
        assert!(diagnostics(&events)[0].starts_with("interactive:1:17:"));
    }
    for command in ["mget", "mset", "mdel", "mhas"] {
        for key in ["(+ 1)", "(eq a a)", "(do)", "(list)", "(map)"] {
            let tail = if command == "mset" {
                " (set calls (+ $calls 1))"
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
        [r#"(0 (map "key" "value"))"#]
    );
    let events = execute(&mut repl, "(mget $missing key (set calls (+ $calls 1)))");
    assert!(diagnostics(&events)[0].contains("undefined variable $missing"));
    assert_eq!(values(&execute(&mut repl, "(get calls)")), ["0"]);
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
        "map", "mget", "mset", "mdel", "mhas", "mkeys", "mvals", "mlen",
    ] {
        let mut repl = Repl::new();
        let events = execute(&mut repl, &format!("(str (type -t {command}))"));
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        assert_eq!(values(&events), [r#""builtin""#]);
        let events = execute(&mut repl, &format!("(defn {command} () (exit 9))"));
        assert!(diagnostics(&events)[0].contains("function name is reserved"));
    }
}
