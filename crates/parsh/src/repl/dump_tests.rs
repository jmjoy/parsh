use super::*;

fn outputs(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Output { stream, bytes } if *stream == Stream::Stdout => {
                Some(String::from_utf8(bytes.clone()).unwrap())
            }
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
fn dump_prints_any_value_using_display_and_returns_nil() {
    let events = execute(
        &mut Repl::new(),
        r#"
        (defstruct Point x y)
        (dump (map name parsh items [#1 #true]))
        (dump (Point #1 "two"))
        (dump (bytes/from-text "ok"))
        (dump (fn () #nil))
        "#,
    );
    assert!(diagnostics(&events).is_empty(), "{events:?}");
    assert_eq!(
        outputs(&events),
        "(map \"name\" \"parsh\" \"items\" (#1 #true))\n\
(Point #1 \"two\")\n\
<bytes 2>\n\
<function anonymous>\n"
    );
    assert!(!events.iter().any(|event| matches!(event, Event::Value(_))));
}

#[test]
fn dump_can_be_captured() {
    let events = execute(
        &mut Repl::new(),
        r#"
        (let result (capture-call dump (map key value)))
        (stdout-text $result)
        "#,
    );
    assert!(diagnostics(&events).is_empty(), "{events:?}");
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Value(value) if value == "\"(map \\\"key\\\" \\\"value\\\")\\n\""
    )));
}
