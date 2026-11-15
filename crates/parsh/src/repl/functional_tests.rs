use super::*;

fn check(source: &str, expected: &[&str]) {
    let result = evaluate(&mut Repl::new(), source);
    assert!(
        !result
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(_))),
        "{result:?}"
    );
    assert_eq!(values(&result), expected, "{source}");
}

fn fails(source: &str, expected: &str) {
    let result = evaluate(&mut Repl::new(), source);
    assert!(
        result
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(message) if message.contains(expected))),
        "{source}: {result:?}"
    );
}

#[test]
fn adapters_preserve_values_variadic_calls_and_composition_order() {
    check(
        r#"
        (let add (partial (function +) #10))
        (let both (compose (fn (n) (* $n #2)) (function +)))
        (let always (constantly {items [#nil #true]}))
        (list (list/map $add [#1 #2]) ($both #2 #3) ($always) ($always a b)
              ((compose) [a #nil]) (identity #nil))
        (list (list/filter (complement even?) [#1 #2 #3 #4])
              ((partial +) #1 #2 #3) ((compose str) #1 #2))
    "#,
        &[
            r#"((#11 #12) #10 (map "items" (#nil #true)) (map "items" (#nil #true)) ("a" #nil) #nil)"#,
            r#"((#1 #3) #6 "12")"#,
        ],
    );
    fails("(partial #1)", "callable");
    fails("(compose + #true)", "callable");
    fails("(complement if)", "not an ordinary callable");
    fails("((complement (fn () #1)))", "predicate must return bool");
    fails("((compose) #1 #2)", "expected 1 arguments");
}

#[test]
fn adapters_capture_callees_once_and_keep_lexical_closures() {
    check(
        r#"
        (defn named (n) (+ $n #1))
        (let saved (partial named #5))
        (defn named (n) (+ $n #100))
        (let calls #0)
        (let f (partial + (do (set calls (+ $calls #1)) #10)))
        (let result ($f #2))
        (list ($saved) $calls $result ($f #3))
        (defn factory (initial)
          (compose (fn (n) (+ $n $initial)) (function +)))
        ((factory #4) #1 #2)
    "#,
        &["(#6 #1 #12 #13)", "#7"],
    );
}

#[test]
fn flat_map_expands_exactly_one_level_and_rejects_scalar_results() {
    check(
        r#"
        (list/flat-map (fn (n) (list $n [$n])) [#1 #2])
        (list/flat-map (fn (n) $missing) [])
    "#,
        &["(#1 (#1) #2 (#2))", "()"],
    );
    fails(
        "(list/flat-map identity [#1])",
        "callback must return a list",
    );
}

#[test]
fn keyed_sort_is_stable_precise_and_calls_each_key_once() {
    check(
        r#"
        (let calls #0)
        (let original [{n #9007199254740993 id large}
                       {n #9007199254740992.0 id float}
                       {n #1 id first} {n #1.0 id second}])
        (let sorted (list/sort-by (fn (item)
          (set calls (+ $calls #1)) (map/get $item n)) $original))
        (list (list/map (fn (item) (map/get $item id)) $sorted)
              $calls (map/get (list/first $original) id))
        (list/sort-by identity [])
    "#,
        &[r#"(("first" "second" "float" "large") #4 "large")"#, "()"],
    );
    fails(
        "(list/sort-by identity [a #1])",
        "sort keys must be all text",
    );
    fails(
        "(list/sort-by (constantly #nil) [a])",
        "sort keys must be all text",
    );
}

#[test]
fn grouping_preserves_group_and_item_order_and_requires_text_keys() {
    check(
        r#"
        (list/group-by (fn (n) (if (even? $n) even odd)) [#3 #2 #1 #4])
        (list/group-by identity [])
    "#,
        &[r#"(map "odd" (#3 #1) "even" (#2 #4))"#, "(map)"],
    );
    fails("(list/group-by identity [#1])", "must return text keys");
}

#[test]
fn prefix_operations_short_circuit_and_partitions_keep_the_tail() {
    check(
        r#"
        (let calls #0)
        (defn prefix (n) (set calls (+ $calls #1)) (< $n #3))
        (list (list/take-while prefix [#1 #2 #3 invalid]) $calls)
        (set calls #0)
        (list (list/drop-while prefix [#1 #2 #3 invalid]) $calls)
        (list (list/take-while even? []) (list/drop-while even? [#2 #4])
              (list/partition #2 [#1 [#2] #3 #4 #5]) (list/partition #3 [])
              (list/partition #9223372036854775807 [a b]))
    "#,
        &[
            "((#1 #2) #3)",
            r#"((#3 "invalid") #3)"#,
            r#"(() () ((#1 (#2)) (#3 #4) (#5)) () (("a" "b")))"#,
        ],
    );
    fails(
        "(list/take-while identity [#1])",
        "predicate must return bool",
    );
    fails(
        "(list/drop-while identity [#nil])",
        "predicate must return bool",
    );
    fails("(list/partition #0 [a])", "size must be positive");
    fails("(list/partition #1.0 [])", "expected an integer");
}

#[test]
fn numeric_predicates_keep_explicit_types_and_sign_rules() {
    check(
        r#"
        (list (even? #-9223372036854775808) (odd? #-3) (zero? #-0.0)
              (positive? #0) (negative? #-0.0) (positive? #0.01) (negative? #-1))
    "#,
        &["(#true #true #true #false #false #true #true)"],
    );
    fails("(even? #2.0)", "expected an integer");
    fails("(odd? 3)", "expected an integer");
    fails("(zero? #false)", "expected a number");
    fails("(positive? 1)", "expected a number");
}

#[test]
fn nested_updates_preserve_copies_order_and_create_missing_maps() {
    check(
        r#"
        (let original {http {retries #2 token #nil} tail yes})
        (let copy $original)
        (let updated (map/update-in $original [http retries] + #1))
        (let added (map/set-in $updated [extra enabled] #true))
        (list $original (eq $copy $original) $added)
        (list (map/update {} count (fn (old) (if (nil? $old) #1 (+ $old #1))))
              (map/update-in {} [a b] (constantly [#1])))
        (let calls #0)
        (list (map/get-in $original [http token] (do (set calls (+ $calls #1)) fallback))
              $calls (map/get-in $original [missing leaf] fallback))
    "#,
        &[
            r#"((map "http" (map "retries" #2 "token" #nil) "tail" "yes") #true (map "http" (map "retries" #3 "token" #nil) "tail" "yes" "extra" (map "enabled" #true)))"#,
            r#"((map "count" #1) (map "a" (map "b" (#1))))"#,
            r#"(#nil #1 "fallback")"#,
        ],
    );
    fails("(map/get-in {} [a])", "map path not found");
    fails("(map/get-in {} [])", "1..128 text keys");
    fails("(map/set-in {} [a #1] b)", "text keys");
    fails("(map/get-in {a #nil} [a b] fallback)", "non-map value");
    fails("(map/update-in {a #1} [a b] identity)", "non-map value");
    fails("(map/get-in {a #1} [a] $missing)", "undefined variable");
}

#[test]
fn update_callbacks_run_once_on_a_snapshot_of_the_map() {
    check(
        r#"
        (let original {n #2})
        (let calls #0)
        (let result (map/update $original n (fn (old extra)
            (set original {n #100}) (set calls (+ $calls #1)) (+ $old $extra)) #3))
        (list $result $original $calls)
    "#,
        &[r#"((map "n" #5) (map "n" #100) #1)"#],
    );
}

#[test]
fn functional_builtins_support_function_values_apply_and_eager_arity_checks() {
    for (name, args, expected) in [
        ("identity", "{a #1}", r#"(map "a" #1)"#),
        ("even?", "#2", "#true"),
        ("odd?", "#3", "#true"),
        ("zero?", "#0.0", "#true"),
        ("positive?", "#1", "#true"),
        ("negative?", "#-1", "#true"),
        ("list/flat-map", "identity [[a] [b]]", r#"("a" "b")"#),
        ("list/sort-by", "identity [#2 #1]", "(#1 #2)"),
        ("list/group-by", "identity [a a]", r#"(map "a" ("a" "a"))"#),
        ("list/take-while", "even? [#2 #3]", "(#2)"),
        ("list/drop-while", "even? [#2 #3]", "(#3)"),
        ("list/partition", "#2 [a b c]", r#"(("a" "b") ("c"))"#),
        ("map/update", "{a #1} a + #2", r#"(map "a" #3)"#),
        ("map/get-in", "{a {b #1}} [a b]", "#1"),
        ("map/set-in", "{} [a b] #1", r#"(map "a" (map "b" #1))"#),
        (
            "map/update-in",
            "{a {b #1}} [a b] + #2",
            r#"(map "a" (map "b" #3))"#,
        ),
    ] {
        check(
            &format!("({name} {args}) ((function {name}) {args}) (apply {name} [{args}])"),
            &[expected, expected, expected],
        );
    }
    for source in [
        "(identity $missing extra)",
        "(constantly $missing extra)",
        "(partial)",
        "(complement $missing extra)",
        "(map/update $missing key)",
        "(list/partition $missing)",
    ] {
        fails(source, "arguments");
    }
}
