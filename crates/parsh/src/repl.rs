//! Shared interactive editing and evaluation for native and browser terminals.
use crate::{
    completion::Completion,
    host::{ExecutionHost, HostError, Stream},
    reader::{Diagnostic, Expr, Kind, Read, Reader, closing_delimiter},
    shell::{Shell, Value},
};
use std::{io, ops::Range, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[cfg(test)]
mod collection_tests;
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

enum Submission {
    Complete(Vec<(Expr, Range<usize>)>),
    Incomplete(char),
    Invalid(Diagnostic),
}

fn submission(source: &str) -> Submission {
    let mut reader = Reader::new(source);
    let mut forms: Vec<(Expr, Range<usize>)> = Vec::new();
    loop {
        let start = reader.offset;
        match reader.read() {
            Read::Form(expr) if matches!(expr.kind, Kind::List(_) | Kind::Comment) => {
                forms.push((expr, start..reader.offset));
            }
            Read::Form(expr) => {
                return Submission::Invalid(Diagnostic {
                    offset: expr.offset,
                    message: "top-level expressions must be calls or collection expressions".into(),
                });
            }
            Read::End => {
                if let Some((_, range)) = forms.last_mut() {
                    range.end = source.len();
                }
                return Submission::Complete(forms);
            }
            Read::Incomplete(diagnostic) => {
                let close = source[diagnostic.offset..]
                    .chars()
                    .next()
                    .and_then(closing_delimiter)
                    .unwrap_or(')');
                return Submission::Incomplete(close);
            }
            Read::ExtraClose(diagnostic) | Read::Error(diagnostic) => {
                return Submission::Invalid(diagnostic);
            }
        }
    }
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

    fn insert(&mut self, ch: char) {
        if closing_delimiter(ch).is_some() && self.body.trim().is_empty() {
            self.explicit = true;
        }
        self.body.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
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
            input.cursor = reader.offset - 1;
        } else {
            input.cursor = input.body.len();
        }
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

/// A gray prompt character, positioned in terminal cells within the visible view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DimmedCell {
    pub row: usize,
    pub column: usize,
    pub character: char,
}

pub(crate) struct View {
    pub(crate) lines: Vec<String>,
    pub(crate) cursor: (usize, usize),
    pub(crate) dimmed_cells: Vec<DimmedCell>,
}

pub(crate) fn view_layout(
    input: &Input,
    search: Option<&str>,
    prefix: &str,
    width: usize,
    rows: usize,
) -> View {
    let mut text = input.source();
    let caret = input.cursor + usize::from(!input.explicit);
    let mut dimmed = Vec::new();
    if !input.explicit && input.body.trim().is_empty() {
        dimmed.push((0, '('));
    }
    if let Submission::Incomplete(close) = submission(&text) {
        dimmed.push((text.len(), close));
        text.push(close);
    }
    let mut rendered = Input {
        body: format!("{prefix}{text}"),
        cursor: prefix.len() + caret,
        explicit: true,
    };
    let (mut lines, mut cursor) = layout(&rendered, width.max(2));
    let mut dimmed_cells = Vec::new();
    for (offset, character) in dimmed {
        rendered.cursor = prefix.len() + offset;
        let (_, (row, column)) = layout(&rendered, width.max(2));
        dimmed_cells.push(DimmedCell {
            row,
            column,
            character,
        });
    }
    if let Some(prompt) = search {
        let status = Input {
            body: prompt.into(),
            cursor: prompt.len(),
            explicit: true,
        };
        let (status_lines, status_cursor) = layout(&status, width.max(2));
        cursor = (lines.len() + status_cursor.0, status_cursor.1);
        lines.extend(status_lines);
    }
    let available = rows.max(3) - 1;
    let first = cursor.0.saturating_sub(available - 1);
    let last = lines.len().min(first + available);
    dimmed_cells.retain(|cell| (first..last).contains(&cell.row));
    for cell in &mut dimmed_cells {
        cell.row -= first;
    }
    View {
        lines: lines[first..last].to_vec(),
        cursor: (cursor.0 - first, cursor.1),
        dimmed_cells,
    }
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

    fn enter(
        &mut self,
        shell: &mut Shell,
        console: &mut impl ConsoleBackend,
    ) -> io::Result<Option<i32>> {
        if self.input.body.trim().is_empty() {
            return Ok(None);
        }
        let mut source = self.input.source();
        let mut parsed = submission(&source);
        if let Submission::Incomplete(close) = parsed {
            let candidate = format!("{source}{close}");
            if let Submission::Complete(forms) = submission(&candidate) {
                source = candidate;
                parsed = Submission::Complete(forms);
            }
        }
        let forms = match parsed {
            Submission::Complete(forms) => forms,
            Submission::Incomplete(_) => {
                self.input.insert('\n');
                return Ok(None);
            }
            Submission::Invalid(diagnostic) => {
                console.draw(&self.input, None)?;
                console.pause()?;
                console.diagnostic(&diagnostic.render("interactive", &source))?;
                self.last_code = 2;
                console.resume()?;
                return Ok(None);
            }
        };
        self.editing.baseline();
        // Commit the exact accepted source.
        console.draw(
            &Input {
                body: source.clone(),
                cursor: source.len(),
                explicit: true,
            },
            None,
        )?;
        console.pause()?;
        for (expr, range) in forms {
            self.history.add(source[range].to_owned());
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
                    break;
                }
            }
        }
        self.input.reset(false);
        self.history.index = None;
        console.resume()?;
        Ok(None)
    }

    pub(crate) fn paste(&mut self, text: &str) {
        self.completion = None;
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        self.editing.break_command();
        if let Some(search) = &mut self.search {
            search.query.push_str(&normalized);
            search.find(&self.history, &self.input, false);
            return;
        }
        let before = self.input.clone();
        if self.input.body.trim().is_empty()
            && normalized
                .trim_start()
                .chars()
                .next()
                .is_some_and(|ch| closing_delimiter(ch).is_some())
        {
            self.input.explicit = true;
        }
        self.input.body.insert_str(self.input.cursor, &normalized);
        self.input.cursor += normalized.len();
        self.input.align_cursor();
        self.editing.record(before, &self.input, false);
    }
}

#[cfg(test)]
fn execute(repl: &mut Repl, source: &str) -> Vec<Event> {
    let mut events = repl.paste(source);
    events.extend(repl.input(Key::Enter));
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserting_brackets_only_edits_and_explicit_source_replaces_the_prompt() {
        let mut input = Input::default();
        for ch in "echo \"x)\" (pwd))".chars() {
            input.insert(ch);
        }
        assert_eq!(input.source(), "(echo \"x)\" (pwd))");
        input.reset(false);
        for ch in "(echo x)".chars() {
            input.insert(ch);
        }
        assert_eq!(input.source(), "(echo x)");
    }

    #[test]
    fn history_recall_preserves_complete_source_and_positions_before_the_outer_closer() {
        for source in [
            "(str hello)",
            "(str (str hello))",
            r#"(str "x)" (; ignored (nested)))"#,
            "(; ignored (; nested))",
            "(str\n  \"甲e\u{301}👩‍💻\")",
            " \n(str hello)\t \n",
            "()",
        ] {
            let mut history = History::default();
            history.add(source.into());
            let mut input = Input {
                body: "str saved".into(),
                cursor: 9,
                explicit: false,
            };
            history.previous(&mut input);
            assert_eq!(input.source(), source);
            let mut reader = Reader::new(source);
            reader.read();
            assert_eq!(input.cursor, reader.offset - 1);
            assert!(input.explicit);
            history.next(&mut input);
            assert_eq!(input.source(), "(str saved");
            assert!(!input.explicit);
            assert_eq!(history.entries, [source]);
        }
    }

    #[test]
    fn prompt_hints_are_dimmed_and_never_become_source() {
        for prefix in ["$ ", "# "] {
            let input = Input::default();
            let view = view_layout(&input, None, prefix, 80, 24);
            assert_eq!(view.lines, [format!("{prefix}()")]);
            assert_eq!(view.cursor, (0, 3));
            assert_eq!(
                view.dimmed_cells,
                [
                    DimmedCell {
                        row: 0,
                        column: 2,
                        character: '('
                    },
                    DimmedCell {
                        row: 0,
                        column: 3,
                        character: ')'
                    },
                ]
            );
            assert_eq!(input.source(), "(");
            let input = Input {
                body: "str 甲".into(),
                cursor: 7,
                explicit: false,
            };
            let view = view_layout(&input, None, prefix, 80, 24);
            assert_eq!(view.lines, [format!("{prefix}(str 甲)")]);
            assert_eq!(view.cursor, (0, 9));
            assert_eq!(
                view.dimmed_cells,
                [DimmedCell {
                    row: 0,
                    column: 9,
                    character: ')'
                },]
            );
            assert_eq!(input.source(), "(str 甲");
        }
    }

    #[test]
    fn complete_history_has_no_padding_or_ghost_closer() {
        let input = Input {
            body: "(str hello)".into(),
            cursor: 10,
            explicit: true,
        };
        let view = view_layout(&input, None, "$ ", 80, 24);
        assert_eq!(view.lines, ["$ (str hello)"]);
        assert_eq!(view.cursor, (0, 12));
        assert!(view.dimmed_cells.is_empty());
        let input = Input {
            cursor: input.body.len(),
            ..input
        };
        assert_eq!(
            view_layout(&input, None, "$ ", 80, 24).lines,
            ["$ (str hello)"]
        );
        assert_eq!(input.source(), "(str hello)");
    }

    #[test]
    fn prompt_cells_wrap_and_clip_to_the_visible_view() {
        let view = view_layout(&Input::default(), None, "$ ", 2, 3);
        assert_eq!(view.lines, ["$ ", "()"]);
        assert_eq!(view.cursor, (1, 1));
        assert_eq!(
            view.dimmed_cells[1],
            DimmedCell {
                row: 1,
                column: 1,
                character: ')',
            }
        );
        let input = Input {
            body: "\n\n\n".into(),
            cursor: 3,
            explicit: false,
        };
        let view = view_layout(&input, None, "$ ", 4, 3);
        assert_eq!(view.lines, ["", ")"]);
        assert_eq!(view.cursor, (1, 0));
        assert_eq!(
            view.dimmed_cells,
            [DimmedCell {
                row: 1,
                column: 0,
                character: ')',
            }]
        );
        assert_eq!(completion_lines(&["(candidate".into()], 80), ["(candidate"]);
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
    Newline,
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
        dimmed_cells: Vec<DimmedCell>,
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
        let view = view_layout(input, search, "$ ", self.columns, self.rows);
        self.events.push(Event::Render {
            lines: view.lines,
            row: view.cursor.0,
            column: view.cursor.1,
            dimmed_cells: view.dimmed_cells,
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
        self.dispatch(|session, _, _| {
            session.paste(source);
            Ok(None)
        })
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
        assert_eq!(values(&execute(&mut repl, "(list $args)")), ["(())"]);
        assert!(
            diagnostics(&execute(&mut repl, "(str $HOME)"))[0].contains("undefined variable $HOME")
        );
        assert!(
            values(&execute(
                &mut repl,
                "(let name world) (export GREETING hello)"
            ))
            .is_empty()
        );
        assert_eq!(
            values(&execute(&mut repl, r#"(str $GREETING ", " $name)"#)),
            ["\"hello, world\""]
        );
        repl.input(Key::Interrupt);
        assert_eq!(values(&execute(&mut repl, "(str $name)")), ["\"world\""]);
        repl.reset();
        assert!(diagnostics(&execute(&mut repl, "(str $name)"))[0].contains("undefined variable"));
    }

    #[test]
    fn type_inspects_syntax_builtins_and_functions_and_captures_output() {
        let mut repl = Repl::new();
        let events = execute(
            &mut repl,
            r#"
            (defn known () (exit 9))
            (type type cd known)
            (str (type -t known))
            (if (type -t unknown) (exit 9) (str missing))
            (type -p type)
            "#,
        );
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        let output: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Output {
                    stream: Stream::Stdout,
                    bytes,
                } => Some(bytes.as_slice()),
                _ => None,
            })
            .flatten()
            .copied()
            .collect();
        assert_eq!(
            output,
            b"type is a shell builtin\ncd is a shell builtin\nknown is a function\n"
        );
        assert_eq!(values(&events), ["\"function\"", "\"missing\""]);
    }

    #[test]
    fn strings_interpolate_variables_calls_and_nested_strings() {
        let events = execute(
            &mut Repl::new(),
            r#"
            (let name parsh)
            (let _名字2 世界)
            (defn name () (str called))
            (list "$name" "$(get name)_suffix" "sum=$(+ 1 2)"
                "$(name)" "nested=$(str "a" (str 'b'))" "$_名字2$name" "" '')
            (list (eq '$name $(+ 1 2)\n' "\$name \$(+ 1 2)\\n")
                (eq '${name}' "${name}") (eq '$1 $!' "$1 $!"))
            (let literal '$missing $(missing)')
            (eq "$literal" '$missing $(missing)')
            (; "$undefined $(missing)")
        "#,
        );
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        assert_eq!(
            values(&events),
            [
                r#"("parsh" "parsh_suffix" "sum=3" "called" "nested=ab" "世界parsh" "" "")"#,
                "(true true true)",
                "true",
            ]
        );
    }

    #[test]
    fn triple_quoted_strings_preserve_whitespace_and_evaluate_interpolations_once() {
        let events = execute(
            &mut Repl::new(),
            r#"
            (let name 世界)
            (let count 0)
            (list """""" '''''')
            (str """
  hello, "$name"
  sum=$(+ 1 2)
""")
            (str '''
  '$missing' $(missing)\n
''')
            (str """$(do (set count (+ $count 1)) $count)$(do (set count (+ $count 1)) $count)""")
            (get count)
            (str """nested=$(str """inner "$name" text""" ''''literal' '' quotes''')""")
            (; """$undefined $(missing) ')'""")
        "#,
        );
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        assert_eq!(
            values(&events),
            [
                r#"("" "")"#,
                r#""\n  hello, \"世界\"\n  sum=3\n""#,
                r#""\n  '\$missing' \$(missing)\\n\n""#,
                r#""12""#,
                "2",
                r#""nested=inner \"世界\" text'literal' '' quotes""#,
            ]
        );
    }

    #[test]
    fn get_preserves_types_and_shares_lexical_and_environment_lookup() {
        let events = execute(
            &mut Repl::new(),
            r#"
            (export GREETING hello)
            (let name outer)
            (let number (+ 2))
            (let flag (eq a a))
            (let nothing (do))
            (let items (list a b))
            (defn read () (get name))
            (defn caller (name) (read))
            (list (caller inner) (get (str na me)) (get number) (get flag)
                (get nothing) (get items) (get GREETING))
            (str "$number|$flag|$nothing")
            (set name changed)
            (read)
        "#,
        );
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        assert_eq!(
            values(&events),
            [
                r#"("outer" "outer" 2 true nil ("a" "b") "hello")"#,
                r#""2|true|""#,
                r#""changed""#,
            ]
        );
    }

    #[test]
    fn interpolations_evaluate_once_in_order_and_at_call_time() {
        let events = execute(
            &mut Repl::new(),
            r#"
            (let count (+ 0))
            (let name before)
            (defn show () (str "$name"))
            (str "$(do (set count (+ $count 1)) $count)$(do (set count (+ $count 1)) $count)")
            (get count)
            (set name after)
            (show)
        "#,
        );
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        assert_eq!(values(&events), [r#""12""#, "2", r#""after""#]);
    }

    #[test]
    fn interpolation_errors_locate_the_operand_and_allow_next_submission() {
        for (source, message, location) in [
            (
                r#"(str "你好 $missing")"#,
                "undefined variable $missing",
                "interactive:1:10:",
            ),
            (
                "(str \"sum=$(/ 1\n 0)\")",
                "division by zero",
                "interactive:2:2:",
            ),
            (
                r#"(str "$args")"#,
                "expected a scalar value, got a list",
                "interactive:1:7:",
            ),
            (
                r#"(str "$(list a)")"#,
                "expected a scalar value, got a list",
                "interactive:1:8:",
            ),
            (
                "(str \"\"\"你好\n  $missing\"\"\")",
                "undefined variable $missing",
                "interactive:2:3:",
            ),
            (
                "(str \"\"\"sum=$(/ 1\n 0)\"\"\")",
                "division by zero",
                "interactive:2:2:",
            ),
        ] {
            let mut repl = Repl::new();
            let events = execute(&mut repl, &format!("{source} (str skipped)"));
            let errors = diagnostics(&events);
            assert_eq!(errors.len(), 1, "{events:?}");
            assert!(errors[0].starts_with(location), "{errors:?}");
            assert!(errors[0].contains(message), "{errors:?}");
            assert!(values(&events).is_empty(), "{events:?}");
            assert_eq!(
                values(&execute(&mut repl, "(str recovered)")),
                [r#""recovered""#]
            );
        }
        for (source, message) in [
            ("(get)", "expected 1 arguments"),
            ("(get a b)", "expected 1 arguments"),
            ("(get (+ 1))", "get expects a text variable name"),
            ("(get bad-name)", "invalid variable name"),
            ("(get missing)", "undefined variable $missing"),
            ("(defn get () (str invalid))", "function name is reserved"),
        ] {
            let events = execute(&mut Repl::new(), source);
            assert!(
                diagnostics(&events)[0].contains(message),
                "{source}: {events:?}"
            );
        }
    }

    #[test]
    fn unfinished_interpolations_wait_for_the_whole_batch() {
        let mut repl = Repl::new();
        repl.paste("(str before) (str \"$(+ 1");
        let events = repl.input(Key::Enter);
        assert!(values(&events).is_empty());
        assert!(diagnostics(&events).is_empty());
        type_text(&mut repl, " 2)\")");
        assert_eq!(values(&repl.input(Key::Enter)), [r#""before""#, r#""3""#]);

        repl.paste("(str 'line");
        assert!(values(&repl.input(Key::Enter)).is_empty());
        type_text(&mut repl, "two')");
        assert_eq!(values(&repl.input(Key::Enter)), [r#""line\ntwo""#]);
    }

    #[test]
    fn unfinished_triple_strings_wait_without_executing_the_batch() {
        for delimiter in ["\"\"\"", "'''"] {
            let mut repl = Repl::new();
            repl.paste(&format!("(str before) (str {delimiter}line"));
            let events = repl.input(Key::Enter);
            assert!(values(&events).is_empty());
            assert!(diagnostics(&events).is_empty());
            type_text(&mut repl, &format!("two{}", &delimiter[..2]));
            let events = repl.input(Key::Enter);
            assert!(values(&events).is_empty());
            assert!(diagnostics(&events).is_empty());
            type_text(&mut repl, &format!("three{delimiter})"));
            let expected = Value::Text(format!("line\ntwo{}\nthree", &delimiter[..2])).display();
            assert_eq!(values(&repl.input(Key::Enter)), [r#""before""#, &expected]);
        }
    }

    #[test]
    fn interpolations_share_the_evaluation_depth_limit() {
        // Match the native main thread's stack when exercising the full depth limit.
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let mut source = "(str done)".to_owned();
                for _ in 0..70 {
                    source = format!("(str \"${source}\")");
                }
                let events = execute(&mut Repl::new(), &source);
                assert!(
                    diagnostics(&events)[0].contains("evaluation nesting exceeds 128"),
                    "{events:?}"
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn displayed_text_can_be_read_without_interpolating_its_dollars() {
        let text = "$name $(missing) \"quoted\" \\ newline\n";
        let displayed = Value::Text(text.into()).display();
        let Read::Form(expr) = Reader::new(&displayed).read() else {
            panic!("{displayed}")
        };
        assert_eq!(expr.kind, Kind::Text(text.into()));
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
            let events = execute(&mut Repl::new(), source);
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
            let events = execute(&mut Repl::new(), source);
            assert!(diagnostics(&events).is_empty(), "{source}: {events:?}");
            assert_eq!(values(&events), [expected], "{source}");
        }
    }

    #[test]
    fn numeric_comparisons_return_booleans_for_numbers_and_numeric_text() {
        for (source, expected) in [
            ("(< 2 10)", "true"),
            ("(< 10 2)", "false"),
            ("(< 2 2.0)", "false"),
            ("(<= 2 2.0)", "true"),
            ("(<= 1.5 2)", "true"),
            ("(<= 2 1.5)", "false"),
            ("(> 10 2)", "true"),
            ("(> 2 10)", "false"),
            ("(> 2.0 2)", "false"),
            ("(>= 2.0 2)", "true"),
            ("(>= 2 1.5)", "true"),
            ("(>= 1.5 2)", "false"),
            ("(!= 001 1.0)", "false"),
            ("(!= 1 1.5)", "true"),
            ("(< -2 -1.5)", "true"),
            ("(> -1 -1.5)", "true"),
            ("(< 0 .5)", "true"),
            ("(> 0 -.5)", "true"),
            ("(>= -0.0 0)", "true"),
            (r#"(< " +001 " "2e0")"#, "true"),
            ("(> (+ 1 2) (/ 5 2))", "true"),
            ("(< 1e308 1.1e308)", "true"),
            ("(> 9007199254740993 9007199254740992.0)", "true"),
            ("(< 9007199254740992.0 9007199254740993)", "true"),
            ("(> -9007199254740992.0 -9007199254740993)", "true"),
            ("(< -9007199254740993 -9007199254740992.0)", "true"),
            ("(< 9223372036854775807 9223372036854775808.0)", "true"),
            ("(> -9223372036854775808 -1e19)", "true"),
            ("(<= -9223372036854775808 -9223372036854775808.0)", "true"),
            ("(!= 9223372036854775807 9223372036854775808.0)", "true"),
            ("(if (> 3 2) (str yes) (unavailable))", r#""yes""#),
            ("(if (< 3 2) (unavailable) (str no))", r#""no""#),
        ] {
            let events = execute(&mut Repl::new(), source);
            assert!(diagnostics(&events).is_empty(), "{source}: {events:?}");
            assert_eq!(values(&events), [expected], "{source}");
        }
    }

    #[test]
    fn numeric_comparisons_evaluate_operands_once_from_left_to_right() {
        let events = execute(
            &mut Repl::new(),
            r#"
            (let count (+))
            (defn next () (set count (+ $count 1)) $count)
            (defn increasing (a b) (< $a $b))
            (list (< (next) (next)) $count
                  (<= (next) (next)) $count
                  (> (next) (next)) $count
                  (>= (next) (next)) $count
                  (!= (next) (next)) $count
                  (increasing (next) (next)) $count)
        "#,
        );
        assert!(diagnostics(&events).is_empty(), "{events:?}");
        assert_eq!(
            values(&events),
            ["(true 2 true 4 false 6 false 8 true 10 true 12)"]
        );
    }

    #[test]
    fn numeric_comparisons_validate_arity_and_locate_invalid_operands() {
        for operator in ["!=", "<", "<=", ">", ">="] {
            for args in ["", "1", "1 2 3"] {
                let source = format!("({operator} {args})");
                let events = execute(&mut Repl::new(), &source);
                let errors = diagnostics(&events);
                assert_eq!(errors.len(), 1, "{source}: {events:?}");
                assert!(
                    errors[0].contains("expected 2 arguments"),
                    "{source}: {errors:?}"
                );
                assert!(values(&events).is_empty(), "{source}: {events:?}");
            }
            for (operand, message) in [
                ("invalid", "invalid number"),
                ("(eq a a)", "expected a number or numeric text"),
                ("(list 1)", "expected a number or numeric text"),
                ("(do)", "expected a number or numeric text"),
                ("9223372036854775808", "integer out of range for i64"),
                ("1e309", "number must be finite"),
            ] {
                let mut repl = Repl::new();
                let source = format!("({operator} 1\n {operand}) (str skipped)");
                let events = execute(&mut repl, &source);
                let errors = diagnostics(&events);
                assert_eq!(errors.len(), 1, "{source}: {events:?}");
                assert!(
                    errors[0].starts_with("interactive:2:2:"),
                    "{source}: {errors:?}"
                );
                assert!(errors[0].contains(message), "{source}: {errors:?}");
                assert!(values(&events).is_empty(), "{source}: {events:?}");
                assert_eq!(
                    values(&execute(&mut repl, "(str recovered)")),
                    [r#""recovered""#]
                );
            }
        }
    }

    #[test]
    fn numeric_values_survive_bindings_functions_and_left_to_right_evaluation() {
        let events = execute(
            &mut Repl::new(),
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
            let events = execute(&mut Repl::new(), source);
            let errors = diagnostics(&events);
            assert_eq!(errors.len(), 1, "{source}: {events:?}");
            assert!(errors[0].contains(message), "{source}: {errors:?}");
            assert!(values(&events).is_empty(), "{source}: {events:?}");
        }
        for operator in ["+", "-", "*", "/", "=", "!=", "<", "<=", ">", ">="] {
            let events = execute(
                &mut Repl::new(),
                &format!("(defn {operator} () (str invalid))"),
            );
            assert!(diagnostics(&events)[0].contains("function name is reserved"));
        }
    }

    #[test]
    fn numeric_diagnostics_locate_the_operand_and_halt_only_the_current_batch() {
        let mut repl = Repl::new();
        let events = execute(&mut repl, "(str before)\n(+ 1\n bad)\n(str skipped)");
        assert_eq!(values(&events), [r#""before""#]);
        let errors = diagnostics(&events);
        assert_eq!(errors.len(), 1, "{events:?}");
        assert!(errors[0].starts_with("interactive:3:2:"), "{errors:?}");
        assert!(errors[0].contains("invalid number"), "{errors:?}");
        assert_eq!(values(&execute(&mut repl, "(+ 2 3)")), ["5"]);
    }

    #[test]
    fn bare_binding_names_and_variable_references_have_distinct_roles() {
        let events = execute(
            &mut Repl::new(),
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
                let events = execute(&mut Repl::new(), &source);
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
            let events = execute(&mut Repl::new(), source);
            let errors = diagnostics(&events);
            assert_eq!(errors.len(), 1, "{source}: {events:?}");
            assert!(errors[0].contains(message), "{source}: {errors:?}");
        }
    }

    #[test]
    fn functions_capture_lexical_scope_and_return_typed_values() {
        let events = execute(
            &mut Repl::new(),
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
        let events = execute(
            &mut Repl::new(),
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
            let events = execute(&mut Repl::new(), source);
            let errors = diagnostics(&events);
            assert_eq!(errors.len(), 1, "{source}: {events:?}");
            assert!(errors[0].starts_with("interactive:1:1:"), "{errors:?}");
            assert!(errors[0].contains("requires native parsh"), "{errors:?}");
            assert!(values(&events).is_empty());
        }
    }

    #[test]
    fn enter_submits_implicit_explicit_and_manually_closed_source() {
        for text in ["str hello", "str hello)", "(str hello", "(str hello)"] {
            let mut repl = Repl::new();
            assert!(values(&type_text(&mut repl, text)).is_empty());
            assert_eq!(values(&repl.input(Key::Enter)), ["\"hello\""]);
            assert_eq!(repl.session.history.entries, ["(str hello)"]);
            assert_eq!(values(&repl.input(Key::Enter)), Vec::<&str>::new());
        }
    }

    #[test]
    fn enter_checks_the_entire_buffer_at_any_cursor_position() {
        for cursor in 0..=9 {
            let mut repl = Repl::new();
            type_text(&mut repl, "str hello");
            repl.session.input.cursor = cursor;
            assert_eq!(values(&repl.input(Key::Enter)), ["\"hello\""]);
            assert!(repl.session.input.body.is_empty());
        }
    }

    #[test]
    fn incomplete_input_only_adds_a_newline_and_never_partial_closers() {
        for text in [
            "str (str hello",
            "str \"hello",
            "str \"hello\\",
            "str (; nested (str x",
        ] {
            let mut repl = Repl::new();
            type_text(&mut repl, text);
            let events = repl.input(Key::Enter);
            assert!(values(&events).is_empty());
            assert!(diagnostics(&events).is_empty());
            assert_eq!(repl.session.input.body, format!("{text}\n"));
            assert!(repl.session.history.entries.is_empty());
        }
        let mut repl = Repl::new();
        type_text(&mut repl, "str (str a");
        repl.input(Key::Home);
        repl.input(Key::Enter);
        assert_eq!(repl.session.input.body, "\nstr (str a");
    }

    #[test]
    fn forced_newlines_and_string_parentheses_follow_the_reader() {
        let mut repl = Repl::new();
        type_text(&mut repl, "str \"x)\" (str y)");
        assert!(values(&repl.input(Key::Newline)).is_empty());
        assert_eq!(values(&repl.input(Key::Enter)), ["\"x)y\""]);
        assert_eq!(repl.session.history.entries, ["(str \"x)\" (str y)\n)"]);
    }

    #[test]
    fn paste_is_one_edit_and_enter_validates_the_whole_batch() {
        let mut repl = Repl::new();
        let events = repl.paste("(let name saved)\r\n(str $name)\r");
        assert!(
            events
                .iter()
                .all(|event| matches!(event, Event::Render { .. }))
        );
        assert!(repl.session.history.entries.is_empty());
        assert_eq!(repl.session.input.body, "(let name saved)\n(str $name)\n");
        assert_eq!(values(&repl.input(Key::Enter)), ["\"saved\""]);
        assert_eq!(
            repl.session.history.entries,
            ["(let name saved)", "\n(str $name)\n"]
        );
        assert_eq!(
            values(&execute(&mut repl, "(str a) (str b")),
            ["\"a\"", "\"b\""]
        );
    }

    #[test]
    fn syntax_errors_retain_the_draft_and_prevent_all_execution() {
        for source in [
            "(let name bad))) (str later)",
            r#"(let name bad) (str "\q")"#,
            "(let name bad) bare",
        ] {
            let mut repl = Repl::new();
            let events = execute(&mut repl, source);
            assert_eq!(diagnostics(&events).len(), 1, "{events:?}");
            assert!(values(&events).is_empty());
            assert_eq!(repl.session.input.body, source);
            assert!(repl.session.history.entries.is_empty());
            assert_eq!(repl.session.last_code, 2);
            repl.input(Key::Undo);
            assert!(repl.session.input.body.is_empty());
            assert!(
                diagnostics(&execute(&mut repl, "(str $name)"))[0].contains("undefined variable")
            );
        }
    }

    #[test]
    fn an_incomplete_batch_waits_without_executing_its_prefix() {
        let mut repl = Repl::new();
        let source = "(let name bad) (str (str unfinished";
        let events = execute(&mut repl, source);
        assert!(values(&events).is_empty());
        assert!(diagnostics(&events).is_empty());
        assert_eq!(repl.session.input.body, format!("{source}\n"));
        assert!(repl.session.history.entries.is_empty());
        repl.input(Key::ClearInput);
        assert!(diagnostics(&execute(&mut repl, "(str $name)"))[0].contains("undefined variable"));
    }

    #[test]
    fn runtime_errors_stop_only_the_submitted_batch() {
        let mut repl = Repl::new();
        let events = execute(&mut repl, "(str before) (str $missing) (str skipped)");
        assert_eq!(values(&events), ["\"before\""]);
        assert_eq!(diagnostics(&events).len(), 1);
        assert_eq!(repl.session.history.entries.len(), 2);
        assert_eq!(
            values(&execute(&mut repl, "(str recovered)")),
            ["\"recovered\""]
        );
    }

    #[test]
    fn complete_history_allows_appending_and_restores_the_implicit_draft() {
        for (previous, next) in [(Key::Up, Key::Down), (Key::Previous, Key::Next)] {
            let mut repl = Repl::new();
            execute(&mut repl, "(str first)(str second)");
            type_text(&mut repl, "str saved");
            for (key, source) in [
                (previous, "(str second)"),
                (previous, "(str first)"),
                (next, "(str second)"),
                (next, "(str saved"),
            ] {
                let events = repl.input(key);
                assert!(
                    events
                        .iter()
                        .all(|event| matches!(event, Event::Render { .. }))
                );
                assert_eq!(repl.session.input.source(), source);
            }
            assert!(!repl.session.input.explicit);
            assert_eq!(values(&repl.input(Key::Enter)), ["\"saved\""]);
            repl.input(previous);
            type_text(&mut repl, " 你好");
            assert_eq!(values(&repl.input(Key::Enter)), ["\"saved你好\""]);
        }
    }

    #[test]
    fn empty_enter_preserves_status_and_does_not_add_history() {
        let mut repl = Repl::new();
        repl.session.last_code = 17;
        repl.paste(" \n\t");
        for _ in 0..3 {
            let events = repl.input(Key::Enter);
            assert!(values(&events).is_empty());
            assert!(diagnostics(&events).is_empty());
            assert_eq!(repl.session.last_code, 17);
            assert!(repl.session.history.entries.is_empty());
        }
    }

    #[test]
    fn explicit_exit_and_eof_close_until_reset() {
        let mut repl = Repl::new();
        assert!(execute(&mut repl, "(exit 7)").contains(&Event::Exit(7)));
        assert!(execute(&mut repl, "(str unreachable)").is_empty());
        repl.reset();
        assert!(repl.input(Key::Eof).contains(&Event::Exit(0)));
        repl.reset();
        assert_eq!(
            values(&execute(&mut repl, "(str restarted)")),
            ["\"restarted\""]
        );
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
        fn commands(&mut self) -> Result<Vec<String>, HostError> {
            Ok(["echo", "pwd", "printenv", "false", "binary"]
                .map(String::from)
                .into())
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
    fn type_uses_host_command_metadata_and_never_native_path_or_command_execution() {
        let mut repl = Repl::with_host(TestHost).unwrap();
        let events = execute(
            &mut repl,
            r#"
            (defn echo () (exit 9))
            (type type echo pwd)
            (type -a echo)
            (type -P echo)
            (type -f echo)
            (str (type -t binary))
            (if (type -t /bin/sh) (exit 9))
            "#,
        );
        assert_eq!(
            outputs(&events),
            concat!(
                "type is a shell builtin\necho is a function\npwd is an external command\n",
                "echo is a function\necho is an external command\necho\necho is an external command\n"
            )
        );
        assert!(events.contains(&Event::Value("\"file\"".into())));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::Diagnostic(_))),
            "{events:?}"
        );
    }

    #[test]
    fn interpolated_host_commands_capture_stdout_and_propagate_failures() {
        let mut repl = Repl::with_host(TestHost).unwrap();
        let events = execute(&mut repl, r#"(str "cwd=$(pwd); $(echo "a b")")"#);
        assert!(outputs(&events).is_empty(), "{events:?}");
        assert!(
            events.contains(&Event::Value(r#""cwd=/home/playground; a b""#.into())),
            "{events:?}"
        );
        for source in [
            r#"(str "$(false)") (echo skipped)"#,
            r#"(str "$(binary)") (echo skipped)"#,
        ] {
            let events = execute(&mut repl, source);
            assert!(outputs(&events).is_empty(), "{events:?}");
            assert!(
                events
                    .iter()
                    .any(|event| matches!(event, Event::Diagnostic(_))),
                "{events:?}"
            );
        }
        assert_eq!(
            outputs(&execute(&mut repl, "(echo recovered)")),
            "recovered\n"
        );
    }

    #[test]
    fn host_commands_capture_through_functions_without_leaking_output() {
        let mut repl = Repl::with_host(TestHost).unwrap();
        let events = execute(
            &mut repl,
            "(defn place () (echo (pwd))) (let path (place)) (echo $path) (str $path)",
        );
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
        let events = execute(
            &mut repl,
            "(export MESSAGE hello) (printenv MESSAGE) (if (false) (echo wrong) (echo right)) (glob *.txt) (cd /tmp) (pwd)",
        );
        assert_eq!(outputs(&events), "hello\nright\n/tmp\n");
        assert!(events.contains(&Event::Value("(\"a.txt\" \"b.txt\")".into())));
        let events = execute(&mut repl, "(glob /)");
        assert!(events.contains(&Event::Value("(\"/\")".into())));
        repl.reset();
        let events = execute(&mut repl, "(pwd) (str $MESSAGE)");
        assert_eq!(outputs(&events), "/home/playground\n");
        assert!(events.iter().any(|event| matches!(event, Event::Diagnostic(text) if text.contains("undefined variable $MESSAGE"))));
    }

    #[test]
    fn failed_host_commands_and_non_utf8_capture_halt_only_the_current_batch() {
        let mut repl = Repl::with_host(TestHost).unwrap();
        let events = execute(&mut repl, "(missing) (echo skipped)");
        assert!(outputs(&events).is_empty());
        assert!(events.iter().any(
            |event| matches!(event, Event::Diagnostic(text) if text.contains("missing command"))
        ));
        let events = execute(&mut repl, "(let bytes (binary)) (echo skipped)");
        assert!(outputs(&events).is_empty());
        assert!(
            events.iter().any(
                |event| matches!(event, Event::Diagnostic(text) if text.contains("not UTF-8"))
            )
        );
        assert_eq!(
            outputs(&execute(&mut repl, "(echo recovered)")),
            "recovered\n"
        );
    }
}

#[cfg(test)]
mod completion_tests;

#[cfg(test)]
mod loop_tests;

#[cfg(test)]
mod map_tests;
