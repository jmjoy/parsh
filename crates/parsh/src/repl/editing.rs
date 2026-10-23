use super::*;
use std::{collections::VecDeque, ops::Range};

#[derive(Default)]
pub(super) struct Editing {
    undo: Vec<Input>,
    typing: bool,
    kills: VecDeque<String>,
    last_kill: bool,
    yank: Option<Yank>,
    prefix: Option<Prefix>,
}

#[derive(Clone, Copy)]
enum Prefix {
    Meta,
    ControlX,
}

struct Yank {
    range: Range<usize>,
    index: usize,
    explicit: bool,
    empty: bool,
}

impl Editing {
    pub(super) fn break_command(&mut self) {
        self.typing = false;
        self.last_kill = false;
        self.yank = None;
        self.prefix = None;
    }

    pub(super) fn baseline(&mut self) {
        self.undo.clear();
        self.break_command();
    }

    pub(super) fn record(&mut self, before: Input, after: &Input, typing: bool) {
        if before.body != after.body {
            if !typing || !self.typing {
                self.undo.push(before);
            }
            self.typing = typing;
        } else if !typing {
            self.typing = false;
        }
    }

    fn kill(&mut self, input: &mut Input, range: Range<usize>, backward: bool) {
        if range.is_empty() {
            return;
        }
        let start = range.start;
        let text: String = input.body.drain(range).collect();
        input.cursor = start;
        if self.last_kill
            && let Some(previous) = self.kills.front_mut()
        {
            if backward {
                previous.insert_str(0, &text);
            } else {
                previous.push_str(&text);
            }
        } else {
            self.kills.push_front(text);
        }
        self.last_kill = true;
    }

    fn yank(&mut self, input: &mut Input, rotate: bool) {
        let mut yank = if rotate {
            let Some(mut yank) = self.yank.take() else {
                return;
            };
            yank.index = (yank.index + 1) % self.kills.len();
            yank
        } else {
            Yank {
                range: input.cursor..input.cursor,
                index: 0,
                explicit: input.explicit,
                empty: input.body.trim().is_empty(),
            }
        };
        let Some(text) = self.kills.get(yank.index) else {
            return;
        };
        let start = yank.range.start;
        // This is an edit, not typed input: a yanked ')' must never submit.
        if yank.empty {
            input.explicit = yank.explicit || text.trim_start().starts_with('(');
        }
        input.body.replace_range(yank.range, text);
        input.cursor = start + text.len();
        yank.range = start..input.cursor;
        self.yank = Some(yank);
    }
}

impl Input {
    pub(super) fn align_cursor(&mut self) {
        // An edit can join formerly separate graphemes across its boundary.
        self.cursor = self
            .body
            .grapheme_indices(true)
            .map(|(offset, _)| offset)
            .chain(std::iter::once(self.body.len()))
            .find(|&offset| offset >= self.cursor)
            .unwrap_or(self.body.len());
    }

    fn word_start(&self, whitespace: bool) -> usize {
        let word = |g: &str| {
            if whitespace {
                !g.chars().all(char::is_whitespace)
            } else {
                g.chars().any(char::is_alphanumeric)
            }
        };
        let mut graphemes = self.body[..self.cursor]
            .grapheme_indices(true)
            .rev()
            .peekable();
        let mut start = self.cursor;
        while graphemes.peek().is_some_and(|(_, g)| !word(g)) {
            start = graphemes.next().unwrap().0;
        }
        while graphemes.peek().is_some_and(|(_, g)| word(g)) {
            start = graphemes.next().unwrap().0;
        }
        start
    }

    fn word_end(&self) -> usize {
        let mut graphemes = self.body[self.cursor..].graphemes(true).peekable();
        let mut end = self.cursor;
        while graphemes
            .peek()
            .is_some_and(|g| !g.chars().any(char::is_alphanumeric))
        {
            end += graphemes.next().unwrap().len();
        }
        while graphemes
            .peek()
            .is_some_and(|g| g.chars().any(char::is_alphanumeric))
        {
            end += graphemes.next().unwrap().len();
        }
        end
    }

    fn transpose(&mut self) {
        let start = self.line_start();
        let end = self.line_end();
        if self.cursor == start {
            return;
        }
        let middle = if self.cursor == end {
            self.body[..end]
                .grapheme_indices(true)
                .next_back()
                .unwrap()
                .0
        } else {
            self.cursor
        };
        if middle == start {
            return;
        }
        let left = self.body[..middle]
            .grapheme_indices(true)
            .next_back()
            .unwrap()
            .0;
        let right = middle + self.body[middle..].graphemes(true).next().unwrap().len();
        let text = format!("{}{}", &self.body[middle..right], &self.body[left..middle]);
        self.body.replace_range(left..right, &text);
        self.cursor = right;
    }
}

pub(super) struct Search {
    pub(super) query: String,
    backward: bool,
    origin: Option<usize>,
    index: Option<usize>,
    preview: Input,
    failed: bool,
}

impl Search {
    pub(super) fn find(&mut self, history: &History, original: &Input, repeated: bool) {
        if self.query.is_empty() {
            self.index = None;
            self.preview = original.clone();
            self.failed = false;
            return;
        }
        let position = self.index.or(self.origin);
        let found = if self.backward {
            let end = position.map_or(history.entries.len(), |index| {
                index + usize::from(!repeated)
            });
            (0..end)
                .rev()
                .find(|&index| history.entries[index].contains(&self.query))
        } else {
            let start =
                position.map_or(history.entries.len(), |index| index + usize::from(repeated));
            (start..history.entries.len())
                .find(|&index| history.entries[index].contains(&self.query))
        };
        self.failed = found.is_none();
        if let Some(index) = found {
            self.index = Some(index);
            history.recall(index, &mut self.preview);
        }
    }

    fn prompt(&self) -> String {
        format!(
            "({}{}-i-search)`{}':",
            if self.failed { "failed " } else { "" },
            if self.backward { "reverse" } else { "forward" },
            self.query
        )
    }
}

impl Session {
    pub(crate) fn draw(&self, console: &mut impl ConsoleBackend) -> io::Result<()> {
        if let Some(search) = &self.search {
            console.draw(&search.preview, Some(&search.prompt()))
        } else {
            console.draw(&self.input, None)
        }
    }

    fn finish_search(&mut self, accept: bool) {
        let Some(search) = self.search.take() else {
            return;
        };
        if !search.query.is_empty() {
            self.last_search = search.query;
        }
        if accept && let Some(index) = search.index {
            if self.history.index.is_none() {
                self.history.draft = Some((self.input.body.clone(), self.input.explicit));
            }
            self.history.index = Some(index);
            self.input = search.preview;
            self.editing.baseline();
        }
    }

    fn search_key(&mut self, key: Key) -> bool {
        let Some(search) = &mut self.search else {
            return false;
        };
        match key {
            Key::Character(ch) => {
                search.query.push(ch);
                search.find(&self.history, &self.input, false);
            }
            Key::Backspace => {
                let end = search
                    .query
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(at, _)| at);
                search.query.truncate(end);
                search.find(&self.history, &self.input, false);
            }
            Key::SearchBackward | Key::SearchForward => {
                search.backward = matches!(key, Key::SearchBackward);
                if search.query.is_empty() {
                    search.query.clone_from(&self.last_search);
                }
                search.find(&self.history, &self.input, true);
            }
            Key::Enter | Key::Escape => self.finish_search(true),
            Key::Abort => self.finish_search(false),
            Key::Ignore => {}
            Key::ClearScreen => return false,
            Key::Interrupt | Key::ClearInput => {
                self.finish_search(false);
                return false;
            }
            _ => {
                self.finish_search(true);
                return false;
            }
        }
        true
    }

    pub(crate) fn key(
        &mut self,
        shell: &mut Shell,
        console: &mut impl ConsoleBackend,
        mut key: Key,
    ) -> io::Result<Option<i32>> {
        if !matches!(key, Key::Tab) {
            self.completion = None;
        }
        if self.search_key(key) {
            return Ok(None);
        }
        if let Some(prefix) = self.editing.prefix.take()
            && !matches!(key, Key::Interrupt | Key::ClearInput | Key::Abort)
        {
            key = match (prefix, key) {
                (Prefix::Meta, Key::Character('b')) => Key::WordLeft,
                (Prefix::Meta, Key::Character('f')) => Key::WordRight,
                (Prefix::Meta, Key::Character('d')) => Key::KillWordForward,
                (Prefix::Meta, Key::Character('y')) => Key::YankPop,
                (Prefix::Meta, Key::Backspace) => Key::KillWordBack,
                (Prefix::ControlX, Key::KillStart) => Key::Undo,
                _ => Key::Ignore,
            };
        }
        let typing = matches!(key, Key::Character(_));
        let killing = matches!(
            key,
            Key::KillStart
                | Key::KillEnd
                | Key::KillWordBack
                | Key::KillWordForward
                | Key::KillWhitespaceWord
        );
        if !typing {
            self.editing.typing = false;
        }
        if !killing && !matches!(key, Key::Escape) {
            self.editing.last_kill = false;
        }
        if !matches!(key, Key::Yank | Key::YankPop | Key::Escape) {
            self.editing.yank = None;
        }
        let before = self.input.clone();
        let history_index = self.history.index;
        let mut outcome = None;
        match key {
            Key::Character(ch) => outcome = self.input.insert(ch, false),
            Key::Enter => {
                self.input.insert('\n', false);
            }
            Key::Tab => self.complete(shell, console)?,
            Key::Left => self.input.left(),
            Key::Right => self.input.right(),
            Key::WordLeft => self.input.cursor = self.input.word_start(false),
            Key::WordRight => self.input.cursor = self.input.word_end(),
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
            Key::KillStart | Key::KillWordBack | Key::KillWhitespaceWord => {
                let start = if matches!(key, Key::KillStart) {
                    self.input.line_start()
                } else {
                    self.input
                        .word_start(matches!(key, Key::KillWhitespaceWord))
                };
                let end = self.input.cursor;
                self.editing.kill(&mut self.input, start..end, true);
            }
            Key::KillEnd | Key::KillWordForward => {
                let start = self.input.cursor;
                let mut end = if matches!(key, Key::KillEnd) {
                    self.input.line_end()
                } else {
                    self.input.word_end()
                };
                if matches!(key, Key::KillEnd) && start == end && end < self.input.body.len() {
                    end += 1; // At a line's end, kill the newline.
                }
                self.editing.kill(&mut self.input, start..end, false);
            }
            Key::Yank | Key::YankPop => self
                .editing
                .yank(&mut self.input, matches!(key, Key::YankPop)),
            Key::Transpose => self.input.transpose(),
            Key::ClearScreen => console.clear_screen()?,
            Key::Undo => {
                if let Some(input) = self.editing.undo.pop() {
                    self.input = input;
                }
            }
            Key::Escape => self.editing.prefix = Some(Prefix::Meta),
            Key::ControlX => self.editing.prefix = Some(Prefix::ControlX),
            Key::Abort | Key::Ignore => {}
            Key::SearchBackward | Key::SearchForward => {
                self.search = Some(Search {
                    query: String::new(),
                    backward: matches!(key, Key::SearchBackward),
                    origin: self.history.index,
                    index: None,
                    preview: self.input.clone(),
                    failed: false,
                });
            }
            Key::Interrupt | Key::ClearInput => {
                if matches!(key, Key::Interrupt) {
                    console.pause()?;
                    console.diagnostic("^C")?;
                    self.last_code = 130;
                    console.resume()?;
                }
                self.input.reset(false);
                self.history.index = None;
                self.history.draft = None;
                self.editing.baseline();
                return Ok(None);
            }
            Key::Eof if self.input.body.is_empty() => {
                console.pause()?;
                return Ok(Some(self.last_code));
            }
            Key::Eof => self.input.delete(),
        }
        self.input.align_cursor();
        if self.input.body.trim().is_empty() {
            self.input.explicit = false;
        }
        if let Some(outcome) = outcome {
            let remainder = self.input.body.split_off(self.input.cursor);
            let code = self.accept(shell, console, outcome, None, &mut false)?;
            if code.is_none() {
                self.input = Input {
                    explicit: remainder.trim_start().starts_with('('),
                    body: remainder,
                    cursor: 0,
                };
            }
            return Ok(code);
        }
        if self.history.index != history_index {
            self.editing.baseline();
        } else if !matches!(key, Key::Undo) {
            self.editing.record(before, &self.input, typing);
        }
        Ok(None)
    }
}
