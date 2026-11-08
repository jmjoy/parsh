//! Evaluator assertions are independent of terminal rendering and input editing.
use super::*;

#[derive(Debug, Default)]
pub(super) struct Evaluation {
    pub values: Vec<String>,
    events: Vec<Event>,
}

impl std::ops::Deref for Evaluation {
    type Target = [Event];
    fn deref(&self) -> &Self::Target {
        &self.events
    }
}

pub(super) fn evaluate(repl: &mut Repl, source: &str) -> Evaluation {
    let mut result = Evaluation::default();
    let mut reader = Reader::new(source);
    loop {
        let expr = match reader.read() {
            Read::End => break,
            Read::Form(expr) => expr,
            Read::Error(error) | Read::ExtraClose(error) | Read::Incomplete(error) => {
                result
                    .events
                    .push(Event::Diagnostic(error.render("interactive", source)));
                break;
            }
        };
        let value = repl.shell.execute(&expr);
        for chunk in repl.shell.take_output() {
            result.events.push(Event::Output {
                stream: chunk.stream,
                bytes: chunk.bytes,
            });
        }
        match value {
            Ok(value) => {
                if !matches!(expr.kind, Kind::Comment) {
                    repl.shell.record_repl_value(value.clone());
                }
                // Data tests assert returned language values; command results have dedicated tests.
                if !matches!(value, Value::Nil)
                    && !matches!(&value, Value::Struct { name, .. } if name == "CallResult")
                {
                    result.values.push(value.display());
                }
            }
            Err(error) => {
                if error.exit {
                    result.events.push(Event::Exit(error.code));
                } else {
                    result.events.push(Event::Diagnostic(
                        Diagnostic {
                            offset: error.offset,
                            message: error.message,
                        }
                        .render("interactive", source),
                    ));
                }
                break;
            }
        }
    }
    result
}

pub(super) fn values(result: &Evaluation) -> Vec<&str> {
    result.values.iter().map(String::as_str).collect()
}

pub(super) fn previous(repl: &mut Repl) -> Value {
    let Read::Form(expr) = Reader::new("(get _)").read() else {
        unreachable!()
    };
    repl.shell.execute(&expr).unwrap()
}

pub(super) fn enter_value(repl: &mut Repl) -> String {
    let events = repl.input(Key::Enter);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(_))),
        "{events:?}"
    );
    previous(repl).display()
}
