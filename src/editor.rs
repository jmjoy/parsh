use std::io::{self, Write};

use crossterm::{
    cursor::{MoveDown, MoveToColumn, MoveUp},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute, queue,
    style::{Color, Print, SetForegroundColor, force_color_output},
    terminal::{self, Clear, ClearType, DisableLineWrap, EnableLineWrap},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    reader::{self, Diagnostic, Kind, Read, Reader},
    shell::{Shell, Value},
};

#[derive(Default)]
struct Input {
    body: String,
    cursor: usize,
    explicit: bool,
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

struct Console {
    output: io::Stderr,
    active: bool,
    rows: usize,
    cursor_row: usize,
}

impl Console {
    fn new() -> io::Result<Self> {
        // Prompt color is an editing cue; keep it even when commands use NO_COLOR.
        force_color_output(true);
        let mut console = Self {
            output: io::stderr(),
            active: false,
            rows: 0,
            cursor_row: 0,
        };
        console.resume()?;
        Ok(console)
    }

    fn resume(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        self.active = true;
        execute!(self.output, EnableBracketedPaste, DisableLineWrap)
    }

    fn pause(&mut self) -> io::Result<()> {
        self.finish()?;
        if self.active {
            execute!(self.output, DisableBracketedPaste, EnableLineWrap)?;
            terminal::disable_raw_mode()?;
            self.active = false;
        }
        Ok(())
    }

    fn draw(&mut self, input: &Input) -> io::Result<()> {
        let (width, height) = terminal::size().unwrap_or((80, 24));
        let (lines, cursor) = layout(input, usize::from(width.max(2)));
        let available = usize::from(height.max(3) - 1);
        let first = cursor.0.saturating_sub(available - 1);
        let last = lines.len().min(first + available);
        queue!(self.output, MoveToColumn(0))?;
        if self.cursor_row > 0 {
            queue!(self.output, MoveUp(self.cursor_row as u16))?;
        }
        queue!(self.output, Clear(ClearType::FromCursorDown))?;
        let dim_prompt = !input.explicit && input.body.trim().is_empty();
        for (index, line) in lines[first..last].iter().enumerate() {
            if index != 0 {
                queue!(self.output, Print("\r\n"))?;
            }
            if dim_prompt && first == 0 && index == 0 {
                queue!(
                    self.output,
                    SetForegroundColor(Color::AnsiValue(244)),
                    Print("("),
                    SetForegroundColor(Color::Reset),
                    Print(&line[1..])
                )?;
            } else {
                queue!(self.output, Print(line))?;
            }
        }
        self.rows = last - first;
        self.cursor_row = cursor.0 - first;
        let up = self.rows - 1 - self.cursor_row;
        if up > 0 {
            queue!(self.output, MoveUp(up as u16))?;
        }
        queue!(self.output, MoveToColumn(cursor.1 as u16))?;
        self.output.flush()
    }

    fn finish(&mut self) -> io::Result<()> {
        if self.rows > 0 {
            let down = self.rows - 1 - self.cursor_row;
            if down > 0 {
                queue!(self.output, MoveDown(down as u16))?;
            }
            queue!(self.output, MoveToColumn(0), Print("\r\n"))?;
            self.rows = 0;
            self.cursor_row = 0;
            self.output.flush()?;
        }
        Ok(())
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        if self.active {
            let _ = execute!(self.output, DisableBracketedPaste, EnableLineWrap);
            let _ = terminal::disable_raw_mode();
        }
    }
}

fn layout(input: &Input, width: usize) -> (Vec<String>, (usize, usize)) {
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

struct Session {
    input: Input,
    history: History,
    last_code: i32,
}

impl Session {
    fn accept(
        &mut self,
        shell: &mut Shell,
        console: &mut Console,
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
                    match shell.execute(&expr) {
                        Ok(value) => {
                            if !matches!(value, Value::Nil | Value::Status(_)) {
                                println!("{}", value.display());
                            }
                            self.last_code = 0;
                        }
                        Err(error) => {
                            if error.exit {
                                return Ok(Some(error.code));
                            }
                            eprintln!(
                                "{}",
                                Diagnostic {
                                    offset: error.offset,
                                    message: error.message
                                }
                                .render("interactive", &source)
                            );
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
                eprintln!(
                    "{}",
                    if let Some((source, _)) = paste {
                        diagnostic.render("paste", source)
                    } else {
                        diagnostic.render("interactive", ")")
                    }
                );
                if !*halted {
                    self.last_code = 2;
                }
            }
            Outcome::Invalid(source, diagnostic) => {
                eprintln!("{}", diagnostic.render("interactive", &source));
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

    fn paste(
        &mut self,
        shell: &mut Shell,
        console: &mut Console,
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

pub fn run(shell: &mut Shell) -> io::Result<i32> {
    let mut console = Console::new()?;
    let mut session = Session {
        input: Input::default(),
        history: History::default(),
        last_code: 0,
    };
    loop {
        console.draw(&session.input)?;
        match event::read()? {
            Event::Paste(text) => {
                if let Some(code) = session.paste(shell, &mut console, &text)? {
                    return Ok(code);
                }
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                let mut outcome = None;
                if control {
                    match key.code {
                        KeyCode::Char('c') => {
                            console.pause()?;
                            eprintln!("^C");
                            session.input.reset(false);
                            session.history.index = None;
                            session.last_code = 130;
                            console.resume()?;
                        }
                        KeyCode::Char('d') if session.input.body.is_empty() => {
                            console.pause()?;
                            return Ok(session.last_code);
                        }
                        KeyCode::Char('d') => session.input.delete(),
                        KeyCode::Char('a') => session.input.cursor = session.input.line_start(),
                        KeyCode::Char('e') => session.input.cursor = session.input.line_end(),
                        KeyCode::Char('p') => session.history.previous(&mut session.input),
                        KeyCode::Char('n') => session.history.next(&mut session.input),
                        KeyCode::Char('u') => {
                            let start = session.input.line_start();
                            session.input.body.drain(start..session.input.cursor);
                            session.input.cursor = start;
                        }
                        KeyCode::Char('k') => {
                            let end = session.input.line_end();
                            session.input.body.drain(session.input.cursor..end);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char(ch) => {
                            outcome = session.input.insert(ch, false);
                        }
                        KeyCode::Enter => {
                            session.input.insert('\n', false);
                        }
                        KeyCode::Tab => {
                            session.input.insert('\t', false);
                        }
                        KeyCode::Left => session.input.left(),
                        KeyCode::Right => session.input.right(),
                        KeyCode::Backspace => session.input.backspace(),
                        KeyCode::Delete => session.input.delete(),
                        KeyCode::Home => session.input.cursor = session.input.line_start(),
                        KeyCode::End => session.input.cursor = session.input.line_end(),
                        KeyCode::Up if !session.input.vertical(false) => {
                            session.history.previous(&mut session.input);
                        }
                        KeyCode::Down if !session.input.vertical(true) => {
                            session.history.next(&mut session.input);
                        }
                        _ => {}
                    }
                }
                if session.input.body.trim().is_empty() {
                    session.input.explicit = false;
                }
                if let Some(outcome) = outcome
                    && let Some(code) =
                        session.accept(shell, &mut console, outcome, None, &mut false)?
                {
                    return Ok(code);
                }
            }
            _ => {}
        }
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
