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
fn json_encodes_scalars_collections_and_structs() {
    assert_values(
        r#"
        (json/encode #nil)
        (json/encode (map name parsh count #2 enabled #true items [a #3]))
        (defstruct Point x y)
        (json/encode (Point #1 "two"))
        (json/decode "{\"name\":\"parsh\",\"items\":[true,null],\"count\":2}")
        "#,
        &[
            r#""null""#,
            r#""{\"name\":\"parsh\",\"count\":2,\"enabled\":true,\"items\":[\"a\",3]}""#,
            r#""{\"x\":1,\"y\":\"two\"}""#,
            r#"(map "name" "parsh" "items" (#true #nil) "count" #2)"#,
        ],
    );
}

#[test]
fn json_functions_work_as_values() {
    assert_values(
        r#"
        (let encode (function json/encode))
        (let decode (function json/decode))
        (list ($encode [a #1]) (apply $decode ["[true,false]"]))
        "#,
        &[r#"("[\"a\",1]" (#true #false))"#],
    );
}

#[test]
fn json_rejects_invalid_input_and_runtime_values() {
    assert_diagnostic("(json/decode \"{\")", "json/decode:");
    assert_diagnostic(
        "(json/encode (fn () #nil))",
        "json/encode does not support functions",
    );
    assert_diagnostic("(json/decode #true)", "json/decode expects text or bytes");
}

#[cfg(not(feature = "native"))]
#[test]
fn http_is_explicitly_unsupported_without_native_runtime_support() {
    assert_diagnostic(
        "(http/get \"https://example.com\")",
        "requires native HTTP support",
    );
}
