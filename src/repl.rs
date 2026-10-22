//! Shared interactive editing and evaluation for native and browser terminals.
use crate::{
    reader::{self, Diagnostic, Kind, Read, Reader},
    shell::{Shell, Value},
};
use std::{io, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
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
        if self.explicit {
            self.body.clone()
        } else {
            format!("({}", self.body)
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
        let empty_close = ch == ')' && at_end && self.body.trim().is_empty();
        self.body.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        if ch != ')' || !at_end {
            return None;
        }
        if empty_close {
            return Some(Outcome::ExtraClose);
        }
        let source = self.source();
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
        input.body = self.entries[index].clone();
        input.cursor = input.body.len();
        input.explicit = true;
    }

    fn next(&mut self, input: &mut Input) {
        let Some(index) = self.index else {
            return;
        };
        if index + 1 < self.entries.len() {
            self.index = Some(index + 1);
            input.body = self.entries[index + 1].clone();
            input.explicit = true;
        } else {
            self.index = None;
            let (body, explicit) = self.draft.take().unwrap_or_default();
            input.body = body;
            input.explicit = explicit;
        }
        input.cursor = input.body.len();
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

pub(crate) trait ConsoleBackend {
    fn activity(&mut self, _busy: bool) {}
    fn draw(&mut self, input: &Input) -> io::Result<()>;
    fn pause(&mut self) -> io::Result<()>;
    fn resume(&mut self) -> io::Result<()>;
    fn value(&mut self, text: &str) -> io::Result<()>;
    fn diagnostic(&mut self, text: &str) -> io::Result<()>;
}

#[derive(Default)]
pub(crate) struct Session {
    input: Input,
    history: History,
    last_code: i32,
}

impl Session {
    fn accept(
        &mut self,
        shell: &mut Shell,
        console: &mut impl ConsoleBackend,
        outcome: Outcome,
        paste: Option<(&str, usize)>,
        halted: &mut bool,
    ) -> io::Result<Option<i32>> {
        console.draw(&self.input)?;
        console.pause()?;
        match outcome {
            Outcome::Ready(source) if !*halted => {
                if let Read::Form(expr) = Reader::new(&source).read() {
                    self.history.add(source.clone());
                    console.activity(true);
                    let result = shell.execute(&expr);
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
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
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
                if halted && self.last_code == 130 {
                    self.input.reset(false);
                    break;
                }
            }
        }
        if halted || self.input.body.trim().is_empty() {
            self.input.reset(false);
        }
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
    fn full_source_replaces_implicit_open_and_history_can_submit() {
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
    Interrupt,
    ClearInput,
    Eof,
}

impl Session {
    pub(crate) fn draw(&self, console: &mut impl ConsoleBackend) -> io::Result<()> {
        console.draw(&self.input)
    }

    pub(crate) fn key(
        &mut self,
        shell: &mut Shell,
        console: &mut impl ConsoleBackend,
        key: Key,
    ) -> io::Result<Option<i32>> {
        let mut outcome = None;
        match key {
            Key::Character(ch) => outcome = self.input.insert(ch, false),
            Key::Enter => {
                self.input.insert('\n', false);
            }
            Key::Tab => {
                self.input.insert('\t', false);
            }
            Key::Left => self.input.left(),
            Key::Right => self.input.right(),
            Key::Backspace => self.input.backspace(),
            Key::Delete => self.input.delete(),
            Key::Home => self.input.cursor = self.input.line_start(),
            Key::End => self.input.cursor = self.input.line_end(),
            Key::Up => {
                if !self.input.vertical(false) {
                    self.history.previous(&mut self.input);
                }
            }
            Key::Down => {
                if !self.input.vertical(true) {
                    self.history.next(&mut self.input);
                }
            }
            Key::Previous => self.history.previous(&mut self.input),
            Key::Next => self.history.next(&mut self.input),
            Key::KillStart => {
                let start = self.input.line_start();
                self.input.body.drain(start..self.input.cursor);
                self.input.cursor = start;
            }
            Key::KillEnd => {
                let end = self.input.line_end();
                self.input.body.drain(self.input.cursor..end);
            }
            Key::Interrupt => {
                console.pause()?;
                console.diagnostic("^C")?;
                self.input.reset(false);
                self.history.index = None;
                self.last_code = 130;
                console.resume()?;
            }
            Key::ClearInput => {
                self.input.reset(false);
                self.history.index = None;
            }
            Key::Eof if self.input.body.is_empty() => {
                console.pause()?;
                return Ok(Some(self.last_code));
            }
            Key::Eof => self.input.delete(),
        }
        if self.input.body.trim().is_empty() {
            self.input.explicit = false;
        }
        if let Some(outcome) = outcome {
            return self.accept(shell, console, outcome, None, &mut false);
        }
        Ok(None)
    }
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
    Value(String),
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
    fn draw(&mut self, input: &Input) -> io::Result<()> {
        let (lines, cursor) = layout(input, self.columns.max(2));
        let available = self.rows.max(3) - 1;
        let first = cursor.0.saturating_sub(available - 1);
        let last = lines.len().min(first + available);
        self.events.push(Event::Render {
            lines: lines[first..last].to_vec(),
            row: cursor.0 - first,
            column: cursor.1,
            dim_prompt: !input.explicit && input.body.trim().is_empty(),
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
        let observer = self.observer.take();
        *self = Self::new();
        self.observer = observer;
        self.resize(columns, rows)
    }
}

#[cfg(test)]
mod syntax_tests {
    use super::*;

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
            matches!(events.last(), Some(Event::Render { lines, .. }) if lines == &["(str hello".to_string(), ")".to_string()])
        );
        assert_eq!(values(&repl.input(Key::Character(')'))), ["\"hello\""]);
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
