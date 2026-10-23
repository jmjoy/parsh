mod host;
use parsh::repl::{Event, Key, Repl};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum BrowserEvent {
    Render {
        lines: Vec<String>,
        row: usize,
        column: usize,
        dim_prompt: bool,
    },
    Commit,
    ClearScreen,
    Completions {
        lines: Vec<String>,
    },
    Value {
        text: String,
    },
    Output {
        stream: String,
        bytes: Vec<u8>,
    },
    Diagnostic {
        text: String,
    },
    Exit {
        code: i32,
    },
}

fn events(events: Vec<Event>) -> Result<JsValue, JsValue> {
    let events: Vec<_> = events
        .into_iter()
        .map(|event| match event {
            Event::Render {
                lines,
                row,
                column,
                dim_prompt,
            } => BrowserEvent::Render {
                lines,
                row,
                column,
                dim_prompt,
            },
            Event::Commit => BrowserEvent::Commit,
            Event::ClearScreen => BrowserEvent::ClearScreen,
            Event::Completions { lines } => BrowserEvent::Completions { lines },
            Event::Value(text) => BrowserEvent::Value { text },
            Event::Output { stream, bytes } => BrowserEvent::Output {
                stream: match stream {
                    parsh::host::Stream::Stdout => "stdout",
                    parsh::host::Stream::Stderr => "stderr",
                }
                .into(),
                bytes,
            },
            Event::Diagnostic(text) => BrowserEvent::Diagnostic { text },
            Event::Exit(code) => BrowserEvent::Exit { code },
        })
        .collect();
    serde_wasm_bindgen::to_value(&events).map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub struct Session {
    repl: Repl,
}

impl Default for Session {
    fn default() -> Self {
        Self { repl: Repl::new() }
    }
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new(on_activity: js_sys::Function, host: js_sys::Function) -> Result<Session, JsValue> {
        let repl = Repl::with_host(crate::host::BrowserHost(host))
            .map_err(|error| JsValue::from_str(&error.message))?;
        let mut session = Self { repl };
        session.repl.observe_execution(move |busy| {
            let _ = on_activity.call1(&JsValue::NULL, &JsValue::from_bool(busy));
        });
        Ok(session)
    }

    pub fn input(&mut self, key: &str, text: &str) -> Result<JsValue, JsValue> {
        if key == "text" {
            return events(
                text.chars()
                    .flat_map(|ch| self.repl.input(Key::Character(ch)))
                    .collect(),
            );
        }
        let key = match key {
            "enter" => Key::Enter,
            "tab" => Key::Tab,
            "left" => Key::Left,
            "right" => Key::Right,
            "backspace" => Key::Backspace,
            "delete" => Key::Delete,
            "home" => Key::Home,
            "end" => Key::End,
            "up" => Key::Up,
            "down" => Key::Down,
            "previous" => Key::Previous,
            "next" => Key::Next,
            "kill-start" => Key::KillStart,
            "kill-end" => Key::KillEnd,
            "word-left" => Key::WordLeft,
            "word-right" => Key::WordRight,
            "kill-word-back" => Key::KillWordBack,
            "kill-word-forward" => Key::KillWordForward,
            "kill-whitespace-word" => Key::KillWhitespaceWord,
            "yank" => Key::Yank,
            "yank-pop" => Key::YankPop,
            "transpose" => Key::Transpose,
            "clear-screen" => Key::ClearScreen,
            "search-backward" => Key::SearchBackward,
            "search-forward" => Key::SearchForward,
            "undo" => Key::Undo,
            "escape" => Key::Escape,
            "control-x" => Key::ControlX,
            "abort" => Key::Abort,
            "ignore" => Key::Ignore,
            "interrupt" => Key::Interrupt,
            "clear-input" => Key::ClearInput,
            "eof" => Key::Eof,
            _ => return Err(JsValue::from_str("unknown input event")),
        };
        events(self.repl.input(key))
    }

    pub fn paste(&mut self, source: &str) -> Result<JsValue, JsValue> {
        events(self.repl.paste(source))
    }

    pub fn resize(&mut self, columns: usize, rows: usize) -> Result<JsValue, JsValue> {
        events(self.repl.resize(columns, rows))
    }

    pub fn reset(&mut self) -> Result<JsValue, JsValue> {
        events(self.repl.reset())
    }
}
