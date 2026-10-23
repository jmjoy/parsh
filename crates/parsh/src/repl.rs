//! Shared interactive editing and evaluation for native and browser terminals.
use crate::{
    completion::Completion,
    host::{ExecutionHost, HostError, Stream},
    reader::{self, Diagnostic, Kind, Read, Reader},
    shell::{Shell, Value},
};
use std::{io, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

mod editing;
#[cfg(test)]
mod editing_tests;
use editing::{Editing, Search};

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct Input {
    pub(crate) body: String,
    cursor: usize,
    pub(crate) explicit: bool,
}

enum Outcome {
    Ready(String),
    ExtraClose,
    Invalid(String, Diagnostic),
}

impl Input {
    fn source(&self) -> String {
        self.source_until(self.body.len())
    }

    fn source_until(&self, end: usize) -> String {
        let body = &self.body[..end];
        if self.explicit {
            body.to_owned()
        } else {
            format!("({body}")
        }
    }

    fn reset(&mut self, explicit: bool) {
        self.body.clear();
        self.cursor = 0;
        self.explicit = explicit;
    }

    fn insert(&mut self, ch: char, pasted: bool) -> Option<Outcome> {
        let at_end = self.cursor == self.body.len();
        if ch == ')' && at_end && !pasted && reader::complete(&self.source()) {
            return Some(Outcome::Ready(self.source()));
        }
        if ch == '(' && self.body.trim().is_empty() {
            self.explicit = true;
        }
        let empty_close = ch == ')' && self.body[..self.cursor].trim().is_empty();
        self.body.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        if ch != ')' || (pasted && !at_end) {
            return None;
        }
        if empty_close {
            return Some(Outcome::ExtraClose);
        }
        let source = self.source_until(self.cursor);
        let mut reader = Reader::new(&source);
        match reader.read() {
            Read::Form(expr) if matches!(expr.kind, Kind::List(_) | Kind::Comment) => {
                if reader.read() == Read::End {
                    Some(Outcome::Ready(source))
                } else {
                    None
                }
            }
            Read::Form(expr) => Some(Outcome::Invalid(
                source,
                Diagnostic {
                    offset: expr.offset,
                    message: "top-level expressions must be parenthesized calls".into(),
                },
            )),
            Read::ExtraClose(_) => Some(Outcome::ExtraClose),
            Read::Error(diagnostic) => Some(Outcome::Invalid(source, diagnostic)),
            _ => None,
        }
    }

    fn left(&mut self) {
        self.cursor = self.body[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(offset, _)| offset);
    }

    fn right(&mut self) {
        if let Some(grapheme) = self.body[self.cursor..].graphemes(true).next() {
            self.cursor += grapheme.len();
        }
    }

    fn backspace(&mut self) {
        let old = self.cursor;
        self.left();
        self.body.drain(self.cursor..old);
    }

    fn delete(&mut self) {
        let old = self.cursor;
        self.right();
        self.body.drain(old..self.cursor);
        self.cursor = old;
    }

    fn line_start(&self) -> usize {
        self.body[..self.cursor].rfind('\n').map_or(0, |p| p + 1)
    }
    fn line_end(&self) -> usize {
        self.body[self.cursor..]
            .find('\n')
            .map_or(self.body.len(), |p| self.cursor + p)
    }

    fn vertical(&mut self, down: bool) -> bool {
        let start = self.line_start();
        let end = self.line_end();
        let target = if down {
            if end == self.body.len() {
                return false;
            }
            let next = end + 1;
            next..self.body[next..]
                .find('\n')
                .map_or(self.body.len(), |p| next + p)
        } else {
            if start == 0 {
                return false;
            }
            self.body[..start - 1].rfind('\n').map_or(0, |p| p + 1)..start - 1
        };
        let desired = self.body[start..self.cursor].width();
        let mut width = 0;
        self.cursor = target.start;
        for grapheme in self.body[target].graphemes(true) {
            if width + grapheme.width() > desired {
                break;
            }
            width += grapheme.width();
            self.cursor += grapheme.len();
        }
        true
    }
}

#[derive(Default)]
struct History {
    entries: Vec<String>,
    index: Option<usize>,
    draft: Option<(String, bool)>,
}

impl History {
    fn add(&mut self, source: String) {
        if self.entries.last() != Some(&source) {
            self.entries.push(source);
        }
        self.index = None;
        self.draft = None;
    }

    fn recall(&self, index: usize, input: &mut Input) {
        let source = &self.entries[index];
        input.body.clone_from(source);
        let mut reader = Reader::new(source);
        if let Read::Form(expr) = reader.read()
            && matches!(expr.kind, Kind::List(_) | Kind::Comment)
        {
            // Reopen only the outer form; keep the stored source and whitespace.
            input.body.remove(reader.offset - 1);
        }
        input.cursor = input.body.len();
        input.explicit = true;
    }

    fn previous(&mut self, input: &mut Input) {
        if self.entries.is_empty() {
            return;
        }
        let index = match self.index {
            Some(0) => return,
            Some(index) => index - 1,
            None => {
                self.draft = Some((input.body.clone(), input.explicit));
                self.entries.len() - 1
            }
        };
        self.index = Some(index);
        self.recall(index, input);
    }

    fn next(&mut self, input: &mut Input) {
        let Some(index) = self.index else {
            return;
        };
        if index + 1 < self.entries.len() {
            self.index = Some(index + 1);
            self.recall(index + 1, input);
        } else {
            self.index = None;
            let (body, explicit) = self.draft.take().unwrap_or_default();
            input.body = body;
            input.cursor = input.body.len();
            input.explicit = explicit;
        }
    }
}

pub(crate) fn layout(input: &Input, width: usize) -> (Vec<String>, (usize, usize)) {
    let text = if input.explicit {
        input.body.clone()
    } else {
        format!("({}", input.body)
    };
    let caret = input.cursor + usize::from(!input.explicit);
    let mut lines = vec![String::new()];
    let mut column = 0;
    let mut cursor = (0, 0);
    for (offset, grapheme) in text.grapheme_indices(true) {
        if offset == caret {
            cursor = (lines.len() - 1, column);
        }
        if grapheme == "\n" {
            lines.push(String::new());
            column = 0;
            continue;
        }
        let rendered = if grapheme == "\t" {
            " ".repeat(8 - column % 8)
        } else if grapheme.chars().any(char::is_control) {
            grapheme
                .chars()
                .map(|c| {
                    if c.is_control() {
                        format!("\\u{{{:x}}}", c as u32)
                    } else {
                        c.to_string()
                    }
                })
                .collect::<String>()
        } else if grapheme.width() == 0 {
            format!("◌{grapheme}")
        } else {
            grapheme.into()
        };
        for glyph in rendered.graphemes(true) {
            let (glyph, size) = if glyph.width() > width {
                ("?", 1)
            } else {
                (glyph, glyph.width())
            };
            if column + size > width {
                lines.push(String::new());
                column = 0;
            }
            lines.last_mut().unwrap().push_str(glyph);
            column += size;
            if column == width {
                lines.push(String::new());
                column = 0;
            }
        }
    }
    if caret == text.len() {
        cursor = (lines.len() - 1, column);
    }
    (lines, cursor)
}

pub(crate) fn view_layout(
    input: &Input,
    search: Option<&str>,
    width: usize,
) -> (Vec<String>, (usize, usize)) {
    let (mut lines, mut cursor) = layout(input, width);
    if let Some(prompt) = search {
        let status = Input {
            body: prompt.into(),
            cursor: prompt.len(),
            explicit: true,
        };
        let (status_lines, status_cursor) = layout(&status, width);
        cursor = (lines.len() + status_cursor.0, status_cursor.1);
        lines.extend(status_lines);
    }
    (lines, cursor)
}

pub(crate) fn completion_lines(candidates: &[String], width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for candidate in candidates {
        let input = Input {
            body: candidate.clone(),
            explicit: true,
            ..Input::default()
        };
        let (mut wrapped, _) = layout(&input, width.max(2));
        if wrapped.last().is_some_and(String::is_empty) {
            wrapped.pop();
        }
        lines.extend(wrapped);
    }
    lines
}

pub(crate) trait ConsoleBackend {
    fn activity(&mut self, _busy: bool) {}
    fn draw(&mut self, input: &Input, search: Option<&str>) -> io::Result<()>;
    fn clear_screen(&mut self) -> io::Result<()>;
    fn completions(&mut self, candidates: &[String]) -> io::Result<()>;
    fn pause(&mut self) -> io::Result<()>;
    fn resume(&mut self) -> io::Result<()>;
    fn value(&mut self, text: &str) -> io::Result<()>;
    fn diagnostic(&mut self, text: &str) -> io::Result<()>;
    fn output(&mut self, _stream: Stream, _bytes: &[u8]) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct Session {
    input: Input,
    history: History,
    last_code: i32,
    completion: Option<(String, usize)>,
    editing: Editing,
    search: Option<Search>,
    last_search: String,
}

impl Session {
    fn complete(&mut self, shell: &mut Shell, console: &mut impl ConsoleBackend) -> io::Result<()> {
        let repeated = self
            .completion
            .as_ref()
            .is_some_and(|(body, cursor)| body == &self.input.body && *cursor == self.input.cursor);
        self.completion = None;
        let implicit = usize::from(!self.input.explicit);
        let source = self.input.source();
        let Some(completion) = Completion::query(shell, &source, self.input.cursor + implicit)
        else {
            return Ok(());
        };
        if repeated && completion.candidates.len() > 1 {
            console.completions(&completion.labels())?;
        } else if let Some(edit) = completion.edit(&source) {
            let start = edit.range.start - implicit;
            self.input
                .body
                .replace_range(start..edit.range.end - implicit, &edit.text);
            self.input.cursor = start + edit.cursor;
        }
        if completion.candidates.len() > 1 {
            self.completion = Some((self.input.body.clone(), self.input.cursor));
        }
        Ok(())
    }

    fn accept(
        &mut self,
        shell: &mut Shell,
        console: &mut impl ConsoleBackend,
        outcome: Outcome,
        paste: Option<(&str, usize)>,
        halted: &mut bool,
    ) -> io::Result<Option<i32>> {
        self.editing.baseline();
        console.draw(&self.input, None)?;
        console.pause()?;
        match outcome {
            Outcome::Ready(source) if !*halted => {
                if let Read::Form(expr) = Reader::new(&source).read() {
                    self.history.add(source.clone());
                    console.activity(true);
                    let result = shell.execute(&expr);
                    for chunk in shell.take_output() {
                        console.output(chunk.stream, &chunk.bytes)?;
                    }
                    console.activity(false);
                    match result {
                        Ok(value) => {
                            if !matches!(value, Value::Nil | Value::Status(_)) {
                                console.value(&value.display())?;
                            }
                            self.last_code = 0;
                        }
                        Err(error) => {
                            if error.exit {
                                return Ok(Some(error.code));
                            }
                            console.diagnostic(
                                &Diagnostic {
                                    offset: error.offset,
                                    message: error.message,
                                }
                                .render("interactive", &source),
                            )?;
                            self.last_code = error.code;
                            *halted = true;
                        }
                    }
                }
            }
            Outcome::ExtraClose => {
                let diagnostic = Diagnostic {
                    offset: paste.map_or(0, |(_, offset)| offset),
                    message: "unexpected ')'".into(),
                };
                console.diagnostic(&if let Some((source, _)) = paste {
                    diagnostic.render("paste", source)
                } else {
                    diagnostic.render("interactive", ")")
                })?;
                if !*halted {
                    self.last_code = 2;
                }
            }
            Outcome::Invalid(source, diagnostic) => {
                console.diagnostic(&diagnostic.render("interactive", &source))?;
                if !*halted {
                    self.last_code = 2;
                }
                *halted = true;
            }
            Outcome::Ready(_) => {}
        }
        self.input.reset(paste.is_some());
        self.history.index = None;
        console.resume()?;
        Ok(None)
    }

    pub(crate) fn paste(
        &mut self,
        shell: &mut Shell,
        console: &mut impl ConsoleBackend,
        text: &str,
    ) -> io::Result<Option<i32>> {
        self.completion = None;
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        self.editing.break_command();
        if let Some(search) = &mut self.search {
            search.query.push_str(&normalized);
            search.find(&self.history, &self.input, false);
            return Ok(None);
        }
        let mut before = self.input.clone();
        if self.input.body.trim().is_empty() && normalized.trim_start().starts_with('(') {
            self.input.explicit = true;
        }
        let mut halted = false;
        for (offset, ch) in normalized.char_indices() {
            if let Some(outcome) = self.input.insert(ch, true) {
                if let Some(code) = self.accept(
                    shell,
                    console,
                    outcome,
                    Some((&normalized, offset)),
                    &mut halted,
                )? {
                    return Ok(Some(code));
                }
                before = Input::default();
                if halted && self.last_code == 130 {
                    self.input.reset(false);
                    break;
                }
            }
        }
        if halted || self.input.body.trim().is_empty() {
            self.input.reset(false);
        }
        self.input.align_cursor();
        self.editing.record(before, &self.input, false);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_outer_closer_submits() {
        let mut input = Input::default();
        for ch in "echo \"x)\" (pwd)\n".chars() {
            assert!(input.insert(ch, false).is_none());
        }
        assert!(
            matches!(input.insert(')', false), Some(Outcome::Ready(source)) if source == "(echo \"x)\" (pwd)\n)")
        );
    }

    #[test]
    fn full_source_replaces_implicit_open_and_complete_source_can_submit() {
        let mut input = Input::default();
        for ch in "(echo x".chars() {
            input.insert(ch, false);
        }
        assert_eq!(input.source(), "(echo x");
        assert!(matches!(input.insert(')', true), Some(Outcome::Ready(_))));
        assert!(
            matches!(input.insert(')', false), Some(Outcome::Ready(source)) if source == "(echo x)")
        );
    }

    #[test]
    fn history_recall_reopens_only_the_outer_form_and_preserves_the_draft() {
        for (source, recalled) in [
            ("(str hello)", "(str hello"),
            ("(str (str hello))", "(str (str hello)"),
            (
                r#"(str "x)" (; ignored (nested)))"#,
                r#"(str "x)" (; ignored (nested))"#,
            ),
            ("(; ignored (; nested))", "(; ignored (; nested)"),
            ("(str\n  \"甲e\u{301}👩‍💻\")", "(str\n  \"甲e\u{301}👩‍💻\""),
            (" \n(str hello)\t \n", " \n(str hello\t \n"),
            ("()", "("),
        ] {
            let mut history = History::default();
            history.add(source.into());
            let mut input = Input {
                body: "(str saved)".into(),
                explicit: true,
                ..Input::default()
            };

            history.previous(&mut input);
            assert_eq!(input.source(), recalled);
            assert_eq!(input.cursor, input.body.len());
            assert!(input.explicit);
            assert_eq!(history.entries, [source]);
            assert!(matches!(input.insert(')', false), Some(Outcome::Ready(_))));

            history.next(&mut input);
            assert_eq!(input.source(), "(str saved)");
            assert_eq!(input.cursor, input.body.len());
            assert!(input.explicit);

            history.previous(&mut input);
            assert_eq!(input.source(), recalled);
            assert_eq!(history.entries, [source]);
        }
    }

    #[test]
    fn extra_closer_does_not_use_a_new_implicit_open() {
        let mut input = Input {
            explicit: true,
            ..Input::default()
        };
        assert!(matches!(input.insert(')', true), Some(Outcome::ExtraClose)));
        input.reset(true);
        assert!(matches!(input.insert(')', true), Some(Outcome::ExtraClose)));
    }

    #[test]
    fn grapheme_editing_and_wrapping() {
        let mut input = Input {
            body: "甲e\u{301}👩‍💻乙".into(),
            ..Input::default()
        };
        input.cursor = input.body.len();
        input.left();
        input.backspace();
        assert_eq!(input.body, "甲e\u{301}乙");
        input.backspace();
        assert_eq!(input.body, "甲乙");
        let (lines, cursor) = layout(&input, 4);
        assert_eq!(lines, ["(甲", "乙"]);
        assert_eq!(cursor, (0, 3));
    }
}

/// Terminal-neutral key events. Parsing and submission stay in Rust.
#[derive(Clone, Copy, Debug)]
pub enum Key {
    Character(char),
    Enter,
    Tab,
    Left,
    Right,
    Backspace,
    Delete,
    Home,
    End,
    Up,
    Down,
    Previous,
    Next,
    KillStart,
    KillEnd,
    WordLeft,
    WordRight,
    KillWordBack,
    KillWordForward,
    KillWhitespaceWord,
    Yank,
    YankPop,
    Transpose,
    ClearScreen,
    SearchBackward,
    SearchForward,
    Undo,
    Escape,
    ControlX,
    Abort,
    Ignore,
    Interrupt,
    ClearInput,
    Eof,
}

/// Events emitted by an isolated syntax session, in terminal display order.
#[derive(Debug, PartialEq)]
pub enum Event {
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
    Value(String),
    Output {
        stream: Stream,
        bytes: Vec<u8>,
    },
    Diagnostic(String),
    Exit(i32),
}

struct EventConsole {
    observer: Option<Rc<dyn Fn(bool)>>,
    columns: usize,
    rows: usize,
    events: Vec<Event>,
}

impl ConsoleBackend for EventConsole {
    fn activity(&mut self, busy: bool) {
        if let Some(observer) = &self.observer {
            observer(busy);
        }
    }
    fn draw(&mut self, input: &Input, search: Option<&str>) -> io::Result<()> {
        let (lines, cursor) = view_layout(input, search, self.columns.max(2));
        let available = self.rows.max(3) - 1;
        let first = cursor.0.saturating_sub(available - 1);
        let last = lines.len().min(first + available);
        self.events.push(Event::Render {
            lines: lines[first..last].to_vec(),
            row: cursor.0 - first,
            column: cursor.1,
            dim_prompt: search.is_none() && !input.explicit && input.body.trim().is_empty(),
        });
        Ok(())
    }
    fn clear_screen(&mut self) -> io::Result<()> {
        self.events.push(Event::ClearScreen);
        Ok(())
    }
    fn completions(&mut self, candidates: &[String]) -> io::Result<()> {
        self.events.push(Event::Commit);
        self.events.push(Event::Completions {
            lines: completion_lines(candidates, self.columns),
        });
        Ok(())
    }
    fn pause(&mut self) -> io::Result<()> {
        self.events.push(Event::Commit);
        Ok(())
    }
    fn resume(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn value(&mut self, text: &str) -> io::Result<()> {
        self.events.push(Event::Value(text.into()));
        Ok(())
    }
    fn diagnostic(&mut self, text: &str) -> io::Result<()> {
        self.events.push(Event::Diagnostic(text.into()));
        Ok(())
    }
    fn output(&mut self, stream: Stream, bytes: &[u8]) -> io::Result<()> {
        self.events.push(Event::Output {
            stream,
            bytes: bytes.into(),
        });
        Ok(())
    }
}

/// A persistent language-only REPL with no host filesystem, environment or processes.
pub struct Repl {
    observer: Option<Rc<dyn Fn(bool)>>,
    shell: Shell,
    session: Session,
    columns: usize,
    rows: usize,
    closed: bool,
}

impl Default for Repl {
    fn default() -> Self {
        Self {
            observer: None,
            shell: Shell::syntax(),
            session: Session::default(),
            columns: 80,
            rows: 24,
            closed: false,
        }
    }
}

impl Repl {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a REPL backed by isolated commands and files.
    pub fn with_host(host: impl ExecutionHost + 'static) -> Result<Self, HostError> {
        Ok(Self {
            shell: Shell::with_host(host)?,
            ..Self::default()
        })
    }

    /// Observe evaluation boundaries without interpreting input outside Rust.
    pub fn observe_execution(&mut self, observer: impl Fn(bool) + 'static) {
        self.observer = Some(Rc::new(observer));
    }

    fn dispatch(
        &mut self,
        operation: impl FnOnce(&mut Session, &mut Shell, &mut EventConsole) -> io::Result<Option<i32>>,
    ) -> Vec<Event> {
        if self.closed {
            return Vec::new();
        }
        let mut console = EventConsole {
            observer: self.observer.clone(),
            columns: self.columns,
            rows: self.rows,
            events: Vec::new(),
        };
        if let Some(code) = operation(&mut self.session, &mut self.shell, &mut console)
            .expect("event console operations cannot fail")
        {
            self.closed = true;
            console.events.push(Event::Exit(code));
        } else {
            self.session
                .draw(&mut console)
                .expect("event console drawing cannot fail");
        }
        console.events
    }

    pub fn input(&mut self, key: Key) -> Vec<Event> {
        self.dispatch(|session, shell, console| session.key(shell, console, key))
    }

    pub fn paste(&mut self, source: &str) -> Vec<Event> {
        self.dispatch(|session, shell, console| session.paste(shell, console, source))
    }

    pub fn resize(&mut self, columns: usize, rows: usize) -> Vec<Event> {
        self.columns = columns.max(2);
        self.rows = rows.max(3);
        self.dispatch(|_, _, _| Ok(None))
    }

    pub fn reset(&mut self) -> Vec<Event> {
        let (columns, rows) = (self.columns, self.rows);
        self.session = Session::default();
        self.closed = false;
        let result = self.shell.reset_session();
        let mut events = Vec::new();
        if let Err(error) = result {
            events.push(Event::Diagnostic(error.message));
        }
        events.extend(self.resize(columns, rows));
        events
    }
}

#[cfg(test)]
mod syntax_tests {
    use super::*;

    fn type_text(repl: &mut Repl, text: &str) -> Vec<Event> {
        text.chars()
            .flat_map(|ch| repl.input(Key::Character(ch)))
            .collect()
    }

    fn values(events: &[Event]) -> Vec<&str> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::Value(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
    fn diagnostics(events: &[Event]) -> Vec<&str> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::Diagnostic(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn syntax_session_has_no_host_environment_and_keeps_its_bindings() {
        let mut repl = Repl::new();
        assert_eq!(values(&repl.paste("(list $args)")), ["(())"]);
        assert!(diagnostics(&repl.paste("(str $HOME)"))[0].contains("undefined variable $HOME"));
        assert!(values(&repl.paste("(let name world) (export GREETING hello)")).is_empty());
        assert_eq!(
            values(&repl.paste(r#"(str $GREETING ", " $name)"#)),
            ["\"hello, world\""]
        );
        repl.input(Key::Interrupt);
        assert_eq!(values(&repl.paste("(str $name)")), ["\"world\""]);
        repl.reset();
        assert!(diagnostics(&repl.paste("(str $name)"))[0].contains("undefined variable"));
    }

    #[test]
    fn arithmetic_parses_text_and_preserves_integer_and_float_results() {
        for (source, expected) in [
            ("(+)", "0"),
            ("(*)", "1"),
            ("(+ 123 2)", "125"),
            (r#"(+ "1.5" 2)"#, "3.5"),
            ("(- 10 3 2)", "5"),
            ("(- 5)", "-5"),
            ("(- -3.5)", "3.5"),
            ("(* 2 3 4)", "24"),
            ("(/ 20 2 4)", "2.5"),
            ("(/ 5 2)", "2.5"),
            ("(/ 8 2)", "4.0"),
            ("(/ 4)", "0.25"),
            ("(+ 1 2.0 3)", "6.0"),
            ("(+ 1.0)", "1.0"),
            ("(* -2)", "-2"),
            (r#"(+ "\n +00123 \t")"#, "123"),
            ("(+ .5 1.)", "1.5"),
            ("(+ 2.5e1 -5)", "20.0"),
            ("(+ 1E-2)", "0.01"),
            ("(+ 0.1 0.2)", "0.30000000000000004"),
            ("(+ -0.0)", "-0.0"),
            ("(+ 9007199254740992 1)", "9007199254740993"),
            ("(+ 9223372036854775807)", "9223372036854775807"),
            ("(+ -9223372036854775808)", "-9223372036854775808"),
            ("(- -9223372036854775808 -1)", "-9223372036854775807"),
            ("(str 00123)", r#""00123""#),
            ("(str (/ 8 2))", r#""4.0""#),
            (
                "(list 123 00123 (+ 1 2) (/ 8 2))",
                r#"("123" "00123" 3 4.0)"#,
            ),
        ] {
            let events = Repl::new().paste(source);
            assert!(diagnostics(&events).is_empty(), "{source}: {events:?}");
            assert_eq!(values(&events), [expected], "{source}");
        }
    }

    #[test]
    fn numeric_equality_is_distinct_from_typed_equality() {
        for (source, expected) in [
            ("(eq 001 1)", "false"),
            ("(= 001 1.0)", "true"),
            ("(eq (+ 1 2) 3)", "false"),
            ("(eq (+ 1 2) (+ 3))", "true"),
            ("(eq (+ 1 2) (+ 3.0))", "false"),
            ("(eq (list (+ 3)) (list 3))", "false"),
            ("(eq (+ 1.0) (+ 1e0))", "true"),
            ("(= (+ 1 2) 3.0)", "true"),
            ("(= 3.0 (+ 1 2))", "true"),
            ("(= 1 1.5)", "false"),
            ("(= -0.0 0)", "true"),
            ("(= (+ 0.1 0.2) 0.3)", "false"),
            ("(= 9007199254740993 9007199254740992.0)", "false"),
            ("(= 9007199254740992.0 9007199254740993)", "false"),
            ("(= 9007199254740993 9007199254740993)", "true"),
            ("(= 9223372036854775807 9223372036854775808.0)", "false"),
            ("(= -9223372036854775808 -9223372036854775808.0)", "true"),
        ] {
            let events = Repl::new().paste(source);
            assert!(diagnostics(&events).is_empty(), "{source}: {events:?}");
            assert_eq!(values(&events), [expected], "{source}");
        }
    }

    #[test]
    fn numeric_values_survive_bindings_functions_and_left_to_right_evaluation() {
        let events = Repl::new().paste(
            r#"
            (let n (+ 1 2))
            (defn increment (n) (+ $n 1))
            (set n (increment $n))
            (list $n (increment $n) (str $n) (= $n 4.0))
            (if (= $n 4) (increment $n) (unavailable))
            (let count (+))
            (defn next () (set count (+ $count 1)) $count)
            (list (+ (next) (next)) $count
                  (- (next) (next)) $count
                  (* (next) (next)) $count
                  (/ (next) (next)) $count
                  (= (next) (next)) $count)
        "#,
        );
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        assert_eq!(
            values(&events),
            [r#"(4 5 "4" true)"#, "5", "(3 2 -1 4 30 6 0.875 8 false 10)"]
        );
    }

    #[test]
    fn numeric_errors_reject_invalid_types_ranges_and_results() {
        for (source, message) in [
            (r#"(+ "")"#, "invalid number"),
            (r#"(+ "1 2")"#, "invalid number"),
            ("(+ 12tail)", "invalid number"),
            ("(+ 0x10)", "invalid number"),
            ("(+ 1_000)", "invalid number"),
            ("(+ 1.2.3)", "invalid number"),
            ("(+ NaN)", "invalid number"),
            ("(+ inf)", "invalid number"),
            ("(+ 9223372036854775808)", "integer out of range for i64"),
            ("(+ -9223372036854775809)", "integer out of range for i64"),
            ("(+ 1e309)", "number must be finite"),
            ("(* 1e308 1e308)", "number must be finite"),
            ("(+ 1e308 1e308)", "number must be finite"),
            ("(- -1e308 1e308)", "number must be finite"),
            ("(/ 1e308 1e-308)", "number must be finite"),
            ("(+ (eq a a))", "expected a number or numeric text"),
            ("(+ (list 1))", "expected a number or numeric text"),
            ("(+ (do))", "expected a number or numeric text"),
            ("(= 1 (eq a a))", "expected a number or numeric text"),
            ("(= 1 no-number)", "invalid number"),
            ("(/ 0)", "division by zero"),
            ("(/ 1 -0.0)", "division by zero"),
            ("(/ -0.0)", "division by zero"),
            ("(+ 9223372036854775807 1)", "integer overflow"),
            ("(- -9223372036854775808 1)", "integer overflow"),
            ("(* 9223372036854775807 2)", "integer overflow"),
            ("(- -9223372036854775808)", "integer overflow"),
            ("(+ 9223372036854775807 1 0.0)", "integer overflow"),
            ("(-)", "expected at least 1 arguments"),
            ("(/)", "expected at least 1 arguments"),
            ("(=)", "expected 2 arguments"),
            ("(= 1)", "expected 2 arguments"),
            ("(= 1 1 1)", "expected 2 arguments"),
            (
                "(if (+ 0) (str yes))",
                "a condition must be a boolean or command status",
            ),
        ] {
            let events = Repl::new().paste(source);
            let errors = diagnostics(&events);
            assert_eq!(errors.len(), 1, "{source}: {events:?}");
            assert!(errors[0].contains(message), "{source}: {errors:?}");
            assert!(values(&events).is_empty(), "{source}: {events:?}");
        }
        for operator in ["+", "-", "*", "/", "="] {
            let events = Repl::new().paste(&format!("(defn {operator} () (str invalid))"));
            assert!(diagnostics(&events)[0].contains("function name is reserved"));
        }
    }

    #[test]
    fn numeric_diagnostics_locate_the_operand_and_halt_only_the_current_batch() {
        let mut repl = Repl::new();
        let events = repl.paste("(str before)\n(+ 1\n bad)\n(str skipped)");
        assert_eq!(values(&events), [r#""before""#]);
        let errors = diagnostics(&events);
        assert_eq!(errors.len(), 1, "{events:?}");
        assert!(errors[0].starts_with("interactive:3:2:"), "{errors:?}");
        assert!(errors[0].contains("invalid number"), "{errors:?}");
        assert_eq!(values(&repl.paste("(+ 2 3)")), ["5"]);
    }

    #[test]
    fn bare_binding_names_and_variable_references_have_distinct_roles() {
        let events = Repl::new().paste(
            r#"
            (let name world)
            (let copy $name)
            (set name parsh)
            (export GREETING hello)
            (defn greet (name suffix) (str $GREETING ", " $name $suffix))
            (list name $name $copy (greet $name "!"))
            (let _名字2 保留)
            (str $_名字2)
            (for name (list first last) (str $name))
            (str $name)
        "#,
        );
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        assert_eq!(
            values(&events),
            [
                r#"("name" "parsh" "world" "hello, parsh!")"#,
                r#""保留""#,
                r#""last""#,
                r#""parsh""#,
            ]
        );
    }

    #[test]
    fn binding_targets_must_be_valid_bare_names() {
        for (target, message) in [
            ("$name", "binding must use a bare variable name without '$'"),
            (
                r#""name""#,
                "binding must use a bare variable name without '$'",
            ),
            (
                "(str name)",
                "binding must use a bare variable name without '$'",
            ),
            ("1name", "invalid variable name"),
            ("bad-name", "invalid variable name"),
        ] {
            for source in [
                format!("(let {target} value)"),
                format!("(set {target} value)"),
                format!("(for {target} (list a) (str bad))"),
                format!("(defn f ({target}) (str bad))"),
                format!("(export {target} value)"),
            ] {
                let events = Repl::new().paste(&source);
                let errors = diagnostics(&events);
                assert_eq!(errors.len(), 1, "{source}: {events:?}");
                assert!(errors[0].contains(message), "{source}: {errors:?}");
                assert!(values(&events).is_empty(), "{source}: {events:?}");
            }
        }
        for (source, message) in [
            (
                "(set missing value)",
                "cannot set undefined variable missing",
            ),
            ("(defn f (name name) (str bad))", "duplicate parameter name"),
            (
                "(defn f name (str bad))",
                "function parameters must be a list of bare variable names",
            ),
        ] {
            let events = Repl::new().paste(source);
            let errors = diagnostics(&events);
            assert_eq!(errors.len(), 1, "{source}: {events:?}");
            assert!(errors[0].contains(message), "{source}: {errors:?}");
        }
    }

    #[test]
    fn functions_capture_lexical_scope_and_return_typed_values() {
        let events = Repl::new().paste(
            r#"
            (let name outer)
            (defn show () (str $name))
            (defn call (name) (show))
            (call inner)
            (set name changed)
            (call inner)
            (defn factory (value) (defn captured () (list $value (eq a a))))
            (factory retained)
            (captured)
        "#,
        );
        assert!(diagnostics(&events).is_empty());
        assert_eq!(
            values(&events),
            ["\"outer\"", "\"changed\"", "(\"retained\" true)"]
        );
    }

    #[test]
    fn conditions_comments_and_loops_follow_native_value_semantics() {
        let events = Repl::new().paste(
            r#"
            (; (echo $undefined) (; nested))
            (if (not (eq a b)) (str yes) (echo unavailable))
            (for item (list first second last) (str $item))
            (for item (list) (str $item))
            (do (let word "你好") (str $word "，parsh"))
        "#,
        );
        assert!(diagnostics(&events).is_empty());
        assert_eq!(values(&events), ["\"yes\"", "\"last\"", "\"你好，parsh\""]);
    }

    #[test]
    fn capabilities_fail_before_touching_host_files_or_starting_commands() {
        for source in [
            "(echo hello)",
            "(glob *)",
            "(cd /)",
            "(| (cat) (sort))",
            "(|> out.txt (str x))",
            "(<| input.txt (str x))",
            "(|2>> errors.txt (str x))",
        ] {
            let events = Repl::new().paste(source);
            let errors = diagnostics(&events);
            assert_eq!(errors.len(), 1, "{source}: {events:?}");
            assert!(errors[0].starts_with("interactive:1:1:"), "{errors:?}");
            assert!(errors[0].contains("requires native parsh"), "{errors:?}");
            assert!(values(&events).is_empty());
        }
    }

    #[test]
    fn paste_recovers_extra_closers_and_stops_only_the_failed_batch() {
        let mut repl = Repl::new();
        let events = repl.paste("(str a))) (str b)");
        assert_eq!(values(&events), ["\"a\"", "\"b\""]);
        assert_eq!(diagnostics(&events).len(), 2);
        let events = repl.paste("(str before) (echo unavailable))) (str skipped)");
        assert_eq!(values(&events), ["\"before\""]);
        assert_eq!(diagnostics(&events).len(), 3);
        assert_eq!(values(&repl.paste("(str recovered)")), ["\"recovered\""]);
    }

    #[test]
    fn enter_inserts_a_newline_and_history_requires_a_closer() {
        let mut repl = Repl::new();
        for ch in "str hello".chars() {
            assert!(values(&repl.input(Key::Character(ch))).is_empty());
        }
        assert!(values(&repl.input(Key::Enter)).is_empty());
        assert_eq!(values(&repl.input(Key::Character(')'))), ["\"hello\""]);
        let events = repl.input(Key::Previous);
        assert!(
            matches!(events.last(), Some(Event::Render { lines, row: 1, column: 0, .. }) if lines == &["(str hello".to_string(), String::new()])
        );
        assert!(values(&events).is_empty());
        assert_eq!(values(&repl.input(Key::Character(')'))), ["\"hello\""]);
        assert_eq!(repl.session.history.entries, ["(str hello\n)"]);
    }

    #[test]
    fn history_navigation_allows_appending_and_restores_the_implicit_draft() {
        for (previous, next) in [(Key::Up, Key::Down), (Key::Previous, Key::Next)] {
            let mut repl = Repl::new();
            repl.paste("(str first)(str second)");
            type_text(&mut repl, "str saved");
            for (key, source) in [
                (previous, "(str second"),
                (previous, "(str first"),
                (next, "(str second"),
                (next, "(str saved"),
            ] {
                let events = repl.input(key);
                assert!(
                    events
                        .iter()
                        .all(|event| matches!(event, Event::Render { .. })),
                    "{events:?}"
                );
                assert_eq!(repl.session.input.source(), source);
                assert_eq!(repl.session.input.cursor, repl.session.input.body.len());
            }
            assert!(!repl.session.input.explicit);
            assert_eq!(values(&type_text(&mut repl, ")")), ["\"saved\""]);

            repl.input(previous);
            let events = type_text(&mut repl, " 你好)");
            assert_eq!(values(&events), ["\"saved你好\""]);
            assert!(diagnostics(&events).is_empty());
            assert_eq!(
                repl.session.history.entries,
                [
                    "(str first)",
                    "(str second)",
                    "(str saved)",
                    "(str saved 你好)"
                ]
            );
        }
    }

    #[test]
    fn middle_closer_preserves_drafts_and_commits_only_the_prefix() {
        for (prefix, tail, value) in [
            ("str a", "bc", "\"a\""),
            ("(str a", "bc", "\"a\""),
            ("str 你", "好e\u{301}👩‍💻", "\"你\""),
            ("str a", "\n  bc\n", "\"a\""),
            ("str a", "  \n", "\"a\""),
            ("str a", " \n(str bc)", "\"a\""),
        ] {
            let mut repl = Repl::new();
            assert!(values(&type_text(&mut repl, &format!("{prefix}{tail}"))).is_empty());
            for _ in tail.graphemes(true) {
                repl.input(Key::Left);
            }
            let events = repl.input(Key::Character(')'));
            assert_eq!(values(&events), [value], "{events:?}");
            assert!(diagnostics(&events).is_empty(), "{events:?}");
            let source = if prefix.starts_with('(') {
                format!("{prefix})")
            } else {
                format!("({prefix})")
            };
            assert!(
                matches!(events.first(), Some(Event::Render { lines, .. }) if lines == std::slice::from_ref(&source)),
                "{events:?}"
            );
            assert_eq!(events[1], Event::Commit);
            assert_eq!(repl.session.history.entries, [source]);
            assert_eq!(repl.session.input.body, tail);
            assert_eq!(repl.session.input.cursor, 0);
            let explicit = tail.trim_start().starts_with('(');
            assert_eq!(repl.session.input.explicit, explicit);
            assert!(
                matches!(events.last(), Some(Event::Render { row: 0, column, .. }) if *column == usize::from(!explicit)),
                "{events:?}"
            );
            if reader::complete(tail) {
                assert!(values(&repl.resize(80, 24)).is_empty());
                repl.input(Key::Down);
                repl.input(Key::End);
                assert_eq!(values(&repl.input(Key::Character(')'))), ["\"bc\""]);
            }
        }
    }

    #[test]
    fn preserved_draft_can_be_prefixed_with_the_next_command() {
        let mut repl = Repl::new();
        type_text(&mut repl, "str abc");
        repl.input(Key::Left);
        repl.input(Key::Left);
        assert_eq!(values(&repl.input(Key::Character(')'))), ["\"a\""]);
        type_text(&mut repl, "str ");
        repl.input(Key::End);
        assert_eq!(values(&repl.input(Key::Character(')'))), ["\"bc\""]);
        repl.input(Key::Previous);
        assert_eq!(repl.session.input.source(), "(str bc");
        repl.input(Key::Previous);
        assert_eq!(repl.session.input.source(), "(str a");
    }

    #[test]
    fn middle_closer_in_nested_forms_strings_and_comments_does_not_submit() {
        for (prefix, tail) in [
            ("str (str a", "bc"),
            ("str \"a", "bc\""),
            ("str (; a", "bc"),
            ("(; a (str b", "cd"),
        ] {
            let mut repl = Repl::new();
            type_text(&mut repl, &format!("{prefix}{tail}"));
            for _ in tail.graphemes(true) {
                repl.input(Key::Left);
            }
            let events = repl.input(Key::Character(')'));
            assert!(
                events
                    .iter()
                    .all(|event| matches!(event, Event::Render { .. })),
                "{events:?}"
            );
            assert_eq!(repl.session.input.body, format!("{prefix}){tail}"));
            assert_eq!(repl.session.input.cursor, prefix.len() + 1);
        }
    }

    #[test]
    fn middle_closer_submits_a_complete_top_level_comment() {
        let mut repl = Repl::new();
        type_text(&mut repl, "(; ignoredtail");
        for _ in 0..4 {
            repl.input(Key::Left);
        }
        let events = repl.input(Key::Character(')'));
        assert!(events.contains(&Event::Commit));
        assert!(values(&events).is_empty());
        assert!(diagnostics(&events).is_empty());
        assert_eq!(repl.session.history.entries, ["(; ignored)"]);
        assert_eq!(repl.session.input.body, "tail");
        assert_eq!(repl.session.input.cursor, 0);
    }

    #[test]
    fn middle_closer_keeps_the_draft_after_errors() {
        for (prefix, message) in [
            ("str $missing", "undefined variable $missing"),
            (r#"str "\q""#, "unknown string escape"),
            ("", "unexpected ')'"),
            ("  \n", "unexpected ')'"),
        ] {
            let mut repl = Repl::new();
            type_text(&mut repl, &format!("{prefix}saved"));
            for _ in 0..5 {
                repl.input(Key::Left);
            }
            let events = repl.input(Key::Character(')'));
            assert_eq!(diagnostics(&events).len(), 1, "{events:?}");
            assert!(diagnostics(&events)[0].contains(message), "{events:?}");
            assert!(values(&events).is_empty());
            assert_eq!(repl.session.input.body, "saved");
            assert_eq!(repl.session.input.cursor, 0);
            type_text(&mut repl, "str ");
            repl.input(Key::End);
            assert_eq!(values(&repl.input(Key::Character(')'))), ["\"saved\""]);
        }
    }

    #[test]
    fn pasted_middle_closer_remains_an_edit() {
        let mut repl = Repl::new();
        type_text(&mut repl, "str abc");
        repl.input(Key::Left);
        repl.input(Key::Left);
        let events = repl.paste(")");
        assert!(
            events
                .iter()
                .all(|event| matches!(event, Event::Render { .. })),
            "{events:?}"
        );
        assert_eq!(repl.session.input.body, "str a)bc");
        assert_eq!(repl.session.input.cursor, "str a)".len());
        assert!(repl.session.history.entries.is_empty());
    }

    #[test]
    fn explicit_exit_and_eof_close_until_reset() {
        let mut repl = Repl::new();
        assert!(repl.paste("(exit 7)").contains(&Event::Exit(7)));
        assert!(repl.paste("(str unreachable)").is_empty());
        repl.reset();
        assert!(repl.input(Key::Eof).contains(&Event::Exit(0)));
        repl.reset();
        assert_eq!(values(&repl.paste("(str restarted)")), ["\"restarted\""]);
    }
}

#[cfg(test)]
mod host_tests {
    use super::*;
    use crate::host::{CommandOutput, ExecutionHost, FileMode, HostStreams, OutputChunk};
    use std::{collections::HashMap, ffi::OsString};

    #[derive(Default)]
    struct TestHost;

    impl ExecutionHost for TestHost {
        fn reset(&mut self) -> Result<String, HostError> {
            Ok("/home/playground".into())
        }
        fn run(
            &mut self,
            stages: &[Vec<String>],
            cwd: &str,
            environment: &HashMap<OsString, OsString>,
            _streams: HostStreams,
        ) -> Result<CommandOutput, HostError> {
            let argv = &stages[0];
            let text = match argv[0].as_str() {
                "echo" => format!("{}\n", argv[1..].join(" ")),
                "pwd" => format!("{cwd}\n"),
                "printenv" => format!(
                    "{}\n",
                    environment[&OsString::from(&argv[1])].to_string_lossy()
                ),
                "false" => {
                    return Ok(CommandOutput {
                        code: 1,
                        chunks: vec![],
                    });
                }
                "binary" => {
                    return Ok(CommandOutput {
                        code: 0,
                        chunks: vec![OutputChunk {
                            stream: Stream::Stdout,
                            bytes: vec![255],
                        }],
                    });
                }
                _ => {
                    return Err(HostError {
                        code: 127,
                        message: "missing command".into(),
                    });
                }
            };
            Ok(CommandOutput {
                code: 0,
                chunks: vec![OutputChunk {
                    stream: Stream::Stdout,
                    bytes: text.into_bytes(),
                }],
            })
        }
        fn open(&mut self, _cwd: &str, _path: &str, _mode: FileMode) -> Result<u32, HostError> {
            unreachable!()
        }
        fn close(&mut self, _handle: u32) {
            unreachable!()
        }
        fn chdir(&mut self, _cwd: &str, path: &str) -> Result<String, HostError> {
            Ok(path.into())
        }
        fn paths(&mut self) -> Result<Vec<String>, HostError> {
            Ok([
                "/",
                "/home/playground/a.txt",
                "/home/playground/.hidden.txt",
                "/home/playground/b.txt",
                "/home/playground/sub/",
            ]
            .map(String::from)
            .into())
        }
    }

    fn outputs(events: &[Event]) -> String {
        events
            .iter()
            .filter_map(|event| match event {
                Event::Output {
                    stream: Stream::Stdout,
                    bytes,
                } => Some(String::from_utf8_lossy(bytes)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn host_commands_capture_through_functions_without_leaking_output() {
        let mut repl = Repl::with_host(TestHost).unwrap();
        let events =
            repl.paste("(defn place () (echo (pwd))) (let path (place)) (echo $path) (str $path)");
        assert_eq!(outputs(&events), "/home/playground\n");
        assert!(events.contains(&Event::Value("\"/home/playground\"".into())));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::Diagnostic(_)))
        );
    }

    #[test]
    fn host_conditions_globs_environment_and_reset_keep_language_semantics() {
        let mut repl = Repl::with_host(TestHost).unwrap();
        let events = repl.paste("(export MESSAGE hello) (printenv MESSAGE) (if (false) (echo wrong) (echo right)) (glob *.txt) (cd /tmp) (pwd)");
        assert_eq!(outputs(&events), "hello\nright\n/tmp\n");
        assert!(events.contains(&Event::Value("(\"a.txt\" \"b.txt\")".into())));
        let events = repl.paste("(glob /)");
        assert!(events.contains(&Event::Value("(\"/\")".into())));
        repl.reset();
        let events = repl.paste("(pwd) (str $MESSAGE)");
        assert_eq!(outputs(&events), "/home/playground\n");
        assert!(events.iter().any(|event| matches!(event, Event::Diagnostic(text) if text.contains("undefined variable $MESSAGE"))));
    }

    #[test]
    fn failed_host_commands_and_non_utf8_capture_halt_only_the_current_batch() {
        let mut repl = Repl::with_host(TestHost).unwrap();
        let events = repl.paste("(missing) (echo skipped)");
        assert!(outputs(&events).is_empty());
        assert!(events.iter().any(
            |event| matches!(event, Event::Diagnostic(text) if text.contains("missing command"))
        ));
        let events = repl.paste("(let bytes (binary)) (echo skipped)");
        assert!(outputs(&events).is_empty());
        assert!(
            events.iter().any(
                |event| matches!(event, Event::Diagnostic(text) if text.contains("not UTF-8"))
            )
        );
        assert_eq!(outputs(&repl.paste("(echo recovered)")), "recovered\n");
    }
}

#[cfg(test)]
mod completion_tests;
