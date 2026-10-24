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
        (list (mget m dynamic) (mget m flag) (mget m nothing)
              (mget m items) (mget m nested) (mget m ""))
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
        (list (mset m a updated) (mdel m c) (mset m d fourth))
        (mkeys m)
        (mvals m)
        (list (mlen m) (mhas m a) (mhas m c))
        (mset m c restored)
        (mkeys m)
        (mdel m a)
        (mdel m b)
        (mdel m d)
        (mdel m c)
        (list (mlen m) (mkeys m) (mvals m) (get m))
        "#,
        &[
            "(nil nil nil)",
            r#"("a" "b" "d")"#,
            r#"("updated" "second" "fourth")"#,
            "(3 true false)",
            r#"("a" "b" "d" "c")"#,
            "(0 () () (map))",
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
fn defaults_are_lazy_and_only_apply_to_missing_keys() {
    assert_values(
        r#"
        (let m (map present (do)))
        (let calls (+ 0))
        (list (mget m present (set calls (+ $calls 100)))
              (mget m absent (do (set calls (+ $calls 1)) (list fallback)))
              $calls)
        (mget m added (do (mset m added installed) (mget m added)))
        (mget m added $undefined)
        (list (mget m absent (do)))
        "#,
        &[
            r#"(nil ("fallback") 1)"#,
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
        (defn update () (mset m k captured))
        (defn caller (m) (update) (mget m k))
        (caller (map k argument))
        (list (mget m k) (mget copy k))
        (defn local (m) (mset m k local) (mdel m nested) $m)
        (local $m)
        (let nested (mget m nested))
        (mset nested k changed)
        (mget m nested)
        (mset m nested $nested)
        (mget copy nested)
        (let enumerated (mvals m))
        (defn replace (enumerated) (set enumerated (list replaced)))
        (replace $enumerated)
        (mget m nested)
        "#,
        &[
            r#""argument""#,
            r#"("captured" "outer")"#,
            r#"(map "k" "local")"#,
            r#"(map "k" "original")"#,
            r#"(map "k" "original")"#,
            r#"(map "k" "changed")"#,
        ],
    );
}

#[test]
fn operands_evaluate_once_in_order_and_updates_use_the_current_binding() {
    assert_values(
        r#"
        (let trace (str))
        (let m (map (do (set trace (str $trace k)) start)
                    (do (set trace (str $trace v)) original)))
        (mset m (do (set trace (str $trace k)) (mset m from_key key-side) target)
                (do (set trace (str $trace v)) (mset m from_value value-side) stored))
        (list $trace (mkeys m) (mvals m))
        (mget m (do (mset m found read-side) found))
        (mhas m (do (mset m present has-side) present))
        (mdel m (do (mset m remove me) remove))
        (mkeys m)
        (mset m key (do (set m (map replacement kept)) value))
        (get m)
        "#,
        &[
            r#"("kvkv" ("start" "from_key" "from_value" "target") ("original" "key-side" "value-side" "stored"))"#,
            r#""read-side""#,
            "true",
            r#"("start" "from_key" "from_value" "target" "found" "present")"#,
            r#"(map "replacement" "kept" "key" "value")"#,
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
fn commands_validate_bare_targets_arity_and_nearest_binding_type() {
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
        execute(&mut repl, "(let m (map key value)) (let text scalar)");
        for (target, message) in [
            ("$m", "bare variable name"),
            ("\"m\"", "bare variable name"),
            ("(exit 9)", "bare variable name"),
            ("bad-name", "invalid variable name"),
            ("missing", "undefined variable missing"),
            ("text", "variable text must contain a map"),
        ] {
            let source = format!("({command} {target} {tail})");
            let events = execute(&mut repl, &source);
            assert_eq!(diagnostics(&events).len(), 1, "{source}: {events:?}");
            assert!(
                diagnostics(&events)[0].contains(message),
                "{source}: {events:?}"
            );
        }
        for source in [
            format!("({command})"),
            format!("({command} m {tail} extra extra)"),
        ] {
            let events = execute(&mut repl, &source);
            assert!(
                diagnostics(&events)[0].contains("arguments"),
                "{source}: {events:?}"
            );
        }
        let events = execute(
            &mut repl,
            &format!("(defn shadow (m) ({command} m {tail})) (shadow text)"),
        );
        assert!(
            diagnostics(&events)[0].contains("must contain a map"),
            "{events:?}"
        );
        assert_eq!(values(&execute(&mut repl, "(mget m key)")), [r#""value""#]);
    }
}

#[test]
fn missing_keys_invalid_keys_and_replaced_targets_report_errors_without_overwriting() {
    let mut repl = Repl::new();
    execute(&mut repl, "(let m (map key value)) (let calls (+ 0))");
    for command in ["mget", "mdel"] {
        let source = format!("({command} m missing)");
        let events = execute(&mut repl, &source);
        assert!(diagnostics(&events)[0].contains("map key \"missing\" not found"));
        assert!(diagnostics(&events)[0].starts_with("interactive:1:9:"));
    }
    for command in ["mget", "mset", "mdel", "mhas"] {
        for key in ["(+ 1)", "(eq a a)", "(do)", "(list)", "(map)"] {
            let tail = if command == "mset" {
                " (set calls (+ $calls 1))"
            } else {
                ""
            };
            let events = execute(&mut repl, &format!("({command} m {key}{tail})"));
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
    let events = execute(&mut repl, "(mget missing key (set calls (+ $calls 1)))");
    assert!(diagnostics(&events)[0].contains("undefined variable missing"));
    assert_eq!(values(&execute(&mut repl, "(get calls)")), ["0"]);
    let events = execute(&mut repl, "(mset m key (do (set m scalar) value))");
    assert!(diagnostics(&events)[0].contains("must contain a map"));
    assert_eq!(values(&execute(&mut repl, "(get m)")), [r#""scalar""#]);
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
