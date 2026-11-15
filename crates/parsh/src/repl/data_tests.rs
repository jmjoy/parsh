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

fn assert_values(source: &str, expected: &[&str]) {
    let events = evaluate(&mut Repl::new(), source);
    assert!(diagnostics(&events).is_empty(), "{events:?}");
    assert_eq!(values(&events), expected, "{source}");
}

fn assert_diagnostic(source: &str, expected: &str) {
    let events = evaluate(&mut Repl::new(), source);
    assert!(
        diagnostics(&events)
            .iter()
            .any(|message| message.contains(expected)),
        "{source}: {events:?}"
    );
}

#[test]
fn list_transforms_use_function_values_and_preserve_order() {
    assert_values(
        r#"
        (defn add-mark (value) (str $value "!"))
        (defn keep-yes (value) (eq $value yes))
        (list
          (list/map add-mark [a b])
          (list/filter keep-yes [yes no yes])
          (list/fold (fn (total value) (+ $total $value)) #0 [#1 #2 #3])
          (list/any keep-yes [no yes])
          (list/all keep-yes [yes yes])
          (list/reverse [a b c])
          (list/contains [a #1 #true] #true))
        "#,
        &[r#"(("a!" "b!") ("yes" "yes") #6 #true #true ("c" "b" "a") #true)"#],
    );
}

#[test]
fn list_transforms_handle_empty_inputs_and_predicate_errors() {
    assert_values(
        r#"
        (list/map (fn (value) (str $value)) [])
        (list/filter (fn (value) #true) [])
        (list/any (fn (value) #true) [])
        (list/all (fn (value) #false) [])
        (list/fold (fn (total value) (+ $total $value)) #9 [])
        "#,
        &["()", "()", "#false", "#true", "#9"],
    );
    assert_diagnostic(
        "(list/filter (fn (value) value) [a])",
        "list/filter predicate must return bool",
    );
}

#[test]
fn integer_ranges_handle_direction_steps_and_i64_boundaries() {
    assert_values(
        r#"
        (list (list/range #4) (list/range #2 #5) (list/range #1 #8 #3)
              (list/range #5 #-2 #-2))
        (list (list/range #0) (list/range #-3) (list/range #2 #2)
              (list/range #3 #1) (list/range #1 #3 #-1))
        (list (list/range #9223372036854775806 #9223372036854775807 #2)
              (list/range #-9223372036854775808 #9223372036854775807 #9223372036854775807)
              (list/range #9223372036854775807 #-9223372036854775808 #-9223372036854775808))
        "#,
        &[
            "((#0 #1 #2 #3) (#2 #3 #4) (#1 #4 #7) (#5 #3 #1 #-1))",
            "(() () () () ())",
            "((#9223372036854775806) (#-9223372036854775808 #-1 #9223372036854775806) (#9223372036854775807 #-1))",
        ],
    );
    for (source, message) in [
        ("(list/range #0 #5 #0)", "step must not be zero"),
        ("(list/range 5)", "expected an integer"),
        ("(list/range #0 #5.0)", "expected an integer"),
        ("(list/range #0 #5 #1.0)", "expected an integer"),
        ("(list/range #1000001)", "exceeds 1000000 items"),
        (
            "(list/range #-9223372036854775808 #9223372036854775807)",
            "exceeds 1000000 items",
        ),
    ] {
        assert_diagnostic(source, message);
    }
}

#[test]
fn indexed_list_edits_preserve_copies_and_nested_values() {
    assert_values(
        r#"
        (let original [a {nested [b]} c])
        (let copy $original)
        (let replaced (list/set $original #-2 [#true #nil]))
        (let inserted (list/insert $original #-1 {new #1}))
        (let deleted (list/del $original #0))
        (list (eq $original $copy) $original $replaced $inserted $deleted)
        (list (list/insert [] #0 [a]) (list/insert [b] #0 a)
              (list/insert [a] #1 b) (list/set [a] #-1 b) (list/del [a] #-1))
        "#,
        &[
            r#"(#true ("a" (map "nested" ("b")) "c") ("a" (#true #nil) "c") ("a" (map "nested" ("b")) (map "new" #1) "c") ((map "nested" ("b")) "c"))"#,
            r#"((("a")) ("a" "b") ("a" "b") ("b") ())"#,
        ],
    );
    for source in [
        "(list/set [] #0 a)",
        "(list/set [a] #1 b)",
        "(list/del [] #0)",
        "(list/del [a] #-2)",
        "(list/insert [] #-1 a)",
        "(list/insert [a] #2 b)",
        "(list/insert [a] #-2 b)",
    ] {
        assert_diagnostic(source, "list index out of range");
    }
    for source in [
        "(list/set [a] #0.0 b)",
        "(list/insert [a] 0 b)",
        "(list/del [a] #0.0)",
    ] {
        assert_diagnostic(source, "expected an integer");
    }
}

#[test]
fn list_sort_is_stable_typed_and_precise_for_mixed_numbers() {
    assert_values(
        r#"
        (let original [#3 #1.0 #2 #1 #1.0])
        (list (list/sort $original) $original (list/sort [中 z a]) (list/sort []))
        (list/sort [#9007199254740993 #9007199254740992.0 #-1 #0.5])
        "#,
        &[
            r#"((#1.0 #1 #1.0 #2 #3) (#3 #1.0 #2 #1 #1.0) ("a" "z" "中") ())"#,
            "(#-1 #0.5 #9007199254740992.0 #9007199254740993)",
        ],
    );
    for source in [
        "(list/sort [#1 2])",
        "(list/sort [a #2])",
        "(list/sort [#true])",
        "(list/sort [#nil])",
        "(list/sort [[a]])",
    ] {
        assert_diagnostic(source, "only text or only finite numbers");
    }
}

#[test]
fn unique_and_index_of_use_structural_type_and_value_equality() {
    assert_values(
        r#"
        (list/unique [b a b #1 #1.0 1 #1 #true #nil #nil [a] [a]
                      {a #1 b #2} {b #2 a #1}])
        (list (list/index-of [#1 #1.0 1 #nil #1] #1.0)
              (list/index-of [#1 #1.0 1 #nil #1] #1)
              (list/index-of [#nil] #nil) (list/index-of [a] missing)
              (list/index-of [] a) (list/unique []))
        (defstruct Point x)
        (let f (fn () #nil))
        (list/len (list/unique (list (Point #1) (Point #1) $f $f
                                    (fn () #nil) (quote a) (quote a))))
        "#,
        &[
            r#"("b" "a" #1 #1.0 "1" #true #nil ("a") (map "a" #1 "b" #2))"#,
            "(#1 #0 #0 #nil #nil ())",
            "#4",
        ],
    );
}

#[test]
fn find_short_circuits_returns_original_items_and_requires_booleans() {
    assert_values(
        r#"
        (let calls #0)
        (defn choose (item)
          (set calls (+ $calls #1))
          (> (map/get $item n) #1))
        (let items [{n #1} {n #2 extra [a]} {n invalid}])
        (list (list/find (function choose) $items) $calls)
        (list (list/find (function choose) []) $calls
              (list/find (fn (n) #false) [#1 #2]))
        "#,
        &[r#"((map "n" #2 "extra" ("a")) #2)"#, "(#nil #2 #nil)"],
    );
    assert_diagnostic("(list/find (fn (n) $n) [#1])", "predicate must return bool");
    assert_diagnostic("(list/find (fn (n) $missing) [#1])", "undefined variable");
    assert_diagnostic("(list/find (fn (n) #true) text)", "expects a list");
}

#[test]
fn enumerate_and_zip_keep_types_nesting_and_the_shortest_length() {
    assert_values(
        r#"
        (list (list/enumerate [a {n #1} [b]]) (list/enumerate []))
        (list (list/zip [a b] [#1 #2 #3] [#true])
              (list/zip [[a] #nil]) (list/zip) (list/zip [a] []))
        "#,
        &[
            r#"(((#0 "a") (#1 (map "n" #1)) (#2 ("b"))) ())"#,
            r#"((("a" #1 #true)) ((("a")) (#nil)) () ())"#,
        ],
    );
    assert_diagnostic("(list/zip [] text)", "expected a list");
    assert_diagnostic("(list/enumerate text)", "expected a list");
}

#[test]
fn text_prefix_suffix_and_case_operations_support_unicode_and_empty_patterns() {
    assert_values(
        r#"
        (list (str/starts-with 中文 中) (str/ends-with 中文 文)
              (str/starts-with abc "") (str/ends-with "" "")
              (str/starts-with abc A) (str/ends-with abc .))
        (list (str/lower "ÄBC中文") (str/upper "Straße中文")
              (str/lower "") (str/upper ""))
        "#,
        &[
            r#"(#true #true #true #true #false #false)"#,
            r#"("äbc中文" "STRASSE中文" "" "")"#,
        ],
    );
    for source in [
        "(str/starts-with #1 a)",
        "(str/ends-with a #1)",
        "(str/lower #true)",
        "(str/upper [])",
    ] {
        assert_diagnostic(source, "expected text");
    }
}

#[test]
fn new_value_operations_evaluate_operands_once_in_order_and_check_arity_first() {
    assert_values(
        r#"
        (let trace (str))
        (defn mark (label value) (set trace (str $trace $label)) $value)
        (let items [a b])
        (let changed (list/set (mark l $items) (mark i #0)
                              (do (set items [new]) (mark v [c]))))
        (list $changed $items $trace)
        (set trace (str))
        (list (list/zip (mark a []) (mark b [#1])) $trace)
        (set trace (str))
        (list (str/starts-with (mark t abc) (mark p a)) $trace)
        "#,
        &[
            r#"((("c") "b") ("new") "liv")"#,
            r#"(() "ab")"#,
            r#"(#true "tp")"#,
        ],
    );
    for source in [
        "(list/range $missing #1 #2 #3)",
        "(list/set $missing #0)",
        "(list/insert $missing #0)",
        "(list/del $missing)",
        "(list/sort $missing [])",
        "(list/unique $missing [])",
        "(list/find $missing)",
        "(list/index-of $missing)",
        "(list/enumerate $missing [])",
        "(str/starts-with $missing)",
        "(str/ends-with $missing)",
        "(str/lower $missing extra)",
        "(str/upper $missing extra)",
    ] {
        assert_diagnostic(source, "argument");
    }
}

#[test]
fn maps_merge_and_convert_entries_with_ordered_keys() {
    assert_values(
        r#"
        (let merged (map/merge {a one b two} {b changed c three}) )
        (list $merged
              (map/entries $merged)
              (map/from-entries [[x #1] [y #2]]))
        "#,
        &[
            r#"((map "a" "one" "b" "changed" "c" "three") (("a" "one") ("b" "changed") ("c" "three")) (map "x" #1 "y" #2))"#,
        ],
    );
    assert_diagnostic("(map/from-entries [[a #1] [a #2]])", "duplicate map key");
}

#[test]
fn bytes_support_text_slicing_and_common_encodings() {
    assert_values(
        r#"
        (let bytes (bytes/from-text "hello"))
        (list (bytes/len $bytes)
              (bytes/to-text (bytes/slice $bytes #1 #-1))
              (bytes/to-text (bytes/concat (bytes/from-text "he") (bytes/from-text "llo")))
              (bytes/base64-encode $bytes)
              (bytes/to-text (bytes/base64-decode "aGVsbG8="))
              (bytes/hex-encode $bytes)
              (bytes/to-text (bytes/hex-decode "68656c6c6f")))
        "#,
        &[r#"(#5 "ell" "hello" "aGVsbG8=" "hello" "68656c6c6f" "hello")"#],
    );
}

#[test]
fn bytes_reject_invalid_text_and_encodings() {
    assert_diagnostic(
        "(bytes/to-text (bytes/hex-decode \"ff\"))",
        "bytes/to-text: bytes are not UTF-8",
    );
    assert_diagnostic("(bytes/base64-decode \"invalid!\")", "bytes/base64-decode:");
    assert_diagnostic(
        "(bytes/hex-decode \"abc\")",
        "hex input must contain an even",
    );
}
