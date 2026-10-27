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

fn assert_diagnostic(source: &str, expected: &str) {
    let events = execute(&mut Repl::new(), source);
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
