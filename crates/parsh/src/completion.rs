//! Completion for unfinished source, without evaluating any of it.
use std::ops::Range;

use crate::{
    reader::{self, Quote},
    shell::Shell,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) text: String,
    pub(crate) directory: bool,
}

impl Candidate {
    pub(crate) fn word(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            directory: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Command,
    Path { directories_only: bool },
    Variable,
}

#[derive(Debug, PartialEq, Eq)]
struct Context {
    range: Range<usize>,
    prefix: String,
    kind: Kind,
    quote: Quote,
    closed: bool,
}

#[derive(Default)]
struct Frame {
    head: Option<String>,
    items: usize,
    comment: bool,
    parameters: bool,
}

impl Frame {
    fn declaration(&self) -> bool {
        self.parameters
            || (self.items == 1
                && matches!(
                    self.head.as_deref(),
                    Some("let" | "set" | "export" | "for" | "defn")
                ))
    }

    fn word(&mut self, text: String, quoted: bool) {
        if self.items == 0 {
            self.comment = !quoted && text == ";";
            self.head = Some(text);
        }
        self.items += 1;
    }
}

impl Context {
    fn at(
        frames: &[Frame],
        range: Range<usize>,
        prefix: String,
        quote: Quote,
        closed: bool,
    ) -> Option<Self> {
        let frame = frames.last()?;
        if frames.iter().any(|frame| frame.comment) || frame.declaration() {
            return None;
        }
        let kind = if quote == Quote::None && prefix.starts_with('$') {
            Kind::Variable
        } else if frame.items == 0 {
            Kind::Command
        } else {
            Kind::Path {
                directories_only: frame.head.as_deref() == Some("cd") && frame.items == 1,
            }
        };
        Some(Self {
            range,
            prefix,
            kind,
            quote,
            closed,
        })
    }

    fn parse(source: &str, cursor: usize) -> Option<Self> {
        if !source.is_char_boundary(cursor) {
            return None;
        }
        let mut offset = 0;
        if source.starts_with("#!") {
            offset = source.find('\n').unwrap_or(source.len());
            if cursor < offset {
                return None;
            }
        }
        let mut scanner = Scanner {
            source,
            cursor,
            offset,
            frames: Vec::new(),
            found: false,
            context: None,
        };
        scanner.scan(0, false, false);
        scanner.context
    }
}

/// Scan strings and their embedded calls with the same nesting boundaries as the reader.
struct Scanner<'a> {
    source: &'a str,
    cursor: usize,
    offset: usize,
    frames: Vec<Frame>,
    found: bool,
    context: Option<Context>,
}

impl Scanner<'_> {
    fn peek(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn context(&mut self, range: Range<usize>, prefix: String, quote: Quote, closed: bool) {
        self.found = true;
        self.context = Context::at(&self.frames, range, prefix, quote, closed);
    }

    fn list(&mut self, depth: usize, count_parent: bool) {
        let parameters = count_parent
            && self.frames.last().is_some_and(|parent| {
                parent.parameters || (parent.head.as_deref() == Some("defn") && parent.items == 2)
            });
        self.frames.push(Frame {
            parameters,
            ..Frame::default()
        });
        self.offset += 1;
        self.scan(depth + 1, true, count_parent);
    }

    fn scan(&mut self, depth: usize, nested: bool, count_parent: bool) {
        if depth >= 256 {
            self.found = true;
            return;
        }
        while let Some(ch) = self.peek() {
            if self.found {
                return;
            }
            if self.offset == self.cursor && (ch.is_whitespace() || matches!(ch, '(' | ')')) {
                self.context(self.cursor..self.cursor, String::new(), Quote::None, false);
                return;
            }
            match ch {
                ch if ch.is_whitespace() => self.offset += ch.len_utf8(),
                '(' => self.list(depth, true),
                ')' => {
                    if !nested {
                        self.found = true;
                        return;
                    }
                    let frame = self.frames.pop().unwrap();
                    if count_parent
                        && !frame.comment
                        && let Some(parent) = self.frames.last_mut()
                    {
                        parent.items += 1;
                    }
                    self.offset += 1;
                    return;
                }
                '"' | '\'' => self.string(depth),
                _ => {
                    let start = self.offset;
                    while self.peek().is_some_and(reader::word_character) {
                        self.offset += self.peek().unwrap().len_utf8();
                    }
                    let text = &self.source[start..self.offset];
                    if (start..=self.offset).contains(&self.cursor) {
                        if text == ";" && self.frames.last().is_some_and(|frame| frame.items == 0) {
                            self.found = true;
                            return;
                        }
                        self.context(
                            start..self.offset,
                            self.source[start..self.cursor].into(),
                            Quote::None,
                            false,
                        );
                        return;
                    }
                    if let Some(frame) = self.frames.last_mut() {
                        frame.word(text.into(), false);
                    }
                }
            }
        }
        if !self.found && self.offset == self.cursor {
            self.context(self.cursor..self.cursor, String::new(), Quote::None, false);
        }
    }

    fn string(&mut self, depth: usize) {
        let start = self.offset;
        let quote = Quote::at(&self.source[start..]);
        let delimiter = quote.delimiter();
        self.offset += delimiter.len();
        let content_start = self.offset;
        if self.cursor > start && self.cursor < content_start {
            self.found = true;
            return;
        }
        let mut text = String::new();
        let mut closed = false;
        let mut interpolated = false;
        while let Some(ch) = self.peek() {
            if self.source[self.offset..].starts_with(delimiter) {
                self.offset += delimiter.len();
                closed = true;
                break;
            }
            if quote.interpolates() && ch == '\\' {
                self.offset += 1;
                let Some(escaped) = self.peek().and_then(reader::string_escape) else {
                    self.found = true;
                    return;
                };
                text.push(escaped);
                self.offset += self.peek().unwrap().len_utf8();
            } else if quote.interpolates() && ch == '$' {
                let variable_start = self.offset;
                self.offset += 1;
                if self.peek() == Some('(') {
                    interpolated = true;
                    self.list(depth, false);
                    if self.found {
                        return;
                    }
                } else if self.peek().is_some_and(reader::variable_start)
                    || (self.cursor == self.offset
                        && (self.peek().is_none()
                            || self.source[self.offset..].starts_with(delimiter)))
                {
                    interpolated = true;
                    while self.peek().is_some_and(reader::variable_continue) {
                        self.offset += self.peek().unwrap().len_utf8();
                    }
                    if self.cursor > variable_start && self.cursor <= self.offset {
                        self.context(
                            variable_start..self.offset,
                            self.source[variable_start..self.cursor].into(),
                            quote,
                            false,
                        );
                        if let Some(context) = &mut self.context {
                            context.kind = Kind::Variable;
                        }
                        return;
                    }
                } else {
                    text.push('$');
                }
            } else {
                text.push(ch);
                self.offset += ch.len_utf8();
            }
        }
        if (start..=self.offset).contains(&self.cursor) {
            self.found = true;
            if !interpolated {
                let content_end = self.offset - if closed { delimiter.len() } else { 0 };
                if self.cursor > content_end && self.cursor < self.offset {
                    return;
                }
                let end = self.cursor.max(content_start).min(content_end);
                let prefix = &self.source[content_start..end];
                let prefix = if quote.interpolates() {
                    unescape(prefix)
                } else {
                    Some(prefix.to_owned())
                };
                if let Some(prefix) = prefix {
                    self.context(start..self.offset, prefix, quote, closed);
                }
            }
        } else if let Some(frame) = self.frames.last_mut() {
            frame.word(if interpolated { String::new() } else { text }, true);
        }
    }
}

fn unescape(text: &str) -> Option<String> {
    let mut decoded = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        decoded.push(if ch == '\\' {
            reader::string_escape(chars.next()?)?
        } else {
            ch
        });
    }
    Some(decoded)
}

fn needs_quotes(text: &str) -> bool {
    text.is_empty()
        || text.starts_with('$')
        || text == ";"
        || text.chars().any(|ch| !reader::word_character(ch))
}

fn quoted_content(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            '\\' => "\\\\".into(),
            '"' => "\\\"".into(),
            '$' => "\\$".into(),
            '\n' => "\\n".into(),
            '\r' => "\\r".into(),
            '\t' => "\\t".into(),
            ch => ch.to_string(),
        })
        .collect()
}

fn literal(text: &str) -> String {
    if needs_quotes(text) {
        format!("\"{}\"", quoted_content(text))
    } else {
        text.into()
    }
}

pub(crate) struct Completion {
    context: Context,
    pub(crate) candidates: Vec<Candidate>,
}

pub(crate) struct Edit {
    pub(crate) range: Range<usize>,
    pub(crate) text: String,
    /// Byte offset inside the replacement.
    pub(crate) cursor: usize,
}

impl Completion {
    pub(crate) fn query(shell: &mut Shell, source: &str, cursor: usize) -> Option<Self> {
        let context = Context::parse(source, cursor)?;
        let mut candidates = match context.kind {
            Kind::Command => shell.completion_commands(&context.prefix),
            Kind::Path { directories_only } => {
                shell.completion_paths(&context.prefix, directories_only, false)
            }
            Kind::Variable => shell.completion_variables(&context.prefix),
        };
        candidates.retain(|candidate| {
            !candidate.text.is_empty()
                && candidate.text.starts_with(&context.prefix)
                && !candidate
                    .text
                    .chars()
                    .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
        });
        candidates.sort_by(|a, b| a.text.cmp(&b.text));
        candidates.dedup_by(|a, b| {
            if a.text == b.text {
                b.directory |= a.directory;
                true
            } else {
                false
            }
        });
        Some(Self {
            context,
            candidates,
        })
    }

    pub(crate) fn labels(&self) -> Vec<String> {
        self.candidates
            .iter()
            .map(|candidate| {
                if self.context.kind == Kind::Variable {
                    candidate.text.clone()
                } else {
                    literal(&candidate.text)
                }
            })
            .collect()
    }

    pub(crate) fn edit(&self, source: &str) -> Option<Edit> {
        let first = self.candidates.first()?;
        let unique = self.candidates.len() == 1;
        let text = if unique {
            first.text.as_str()
        } else {
            let length = self.candidates[1..]
                .iter()
                .map(|candidate| {
                    first
                        .text
                        .char_indices()
                        .zip(candidate.text.chars())
                        .take_while(|((_, a), b)| a == b)
                        .last()
                        .map_or(0, |((offset, ch), _)| offset + ch.len_utf8())
                })
                .min()?;
            if length <= self.context.prefix.len() {
                return None;
            }
            &first.text[..length]
        };
        let embedded_variable =
            self.context.kind == Kind::Variable && self.context.quote.interpolates();
        let finished = unique && !first.directory && !embedded_variable;
        let quote = if self.context.kind == Kind::Variable {
            Quote::None
        } else if self.context.quote == Quote::Single && !text.contains('\'') {
            Quote::Single
        } else if self.context.quote == Quote::TripleSingle
            && !text.contains("'''")
            && !text.ends_with('\'')
        {
            Quote::TripleSingle
        } else if self.context.quote == Quote::TripleDouble {
            Quote::TripleDouble
        } else if self.context.quote != Quote::None || needs_quotes(text) {
            Quote::Double
        } else {
            Quote::None
        };
        let content = if quote.interpolates() {
            quoted_content(text)
        } else {
            text.into()
        };
        let mut replacement = format!("{}{content}", quote.delimiter());
        let mut cursor = replacement.len();
        if quote != Quote::None && (finished || self.context.closed) {
            replacement.push_str(quote.delimiter());
            if finished {
                cursor += quote.delimiter().len();
            }
        }
        if finished
            && source[self.context.range.end..]
                .chars()
                .next()
                .is_none_or(|ch| !ch.is_whitespace() && !matches!(ch, '(' | ')'))
        {
            replacement.push(' ');
            cursor = replacement.len();
        }
        Some(Edit {
            range: self.context.range.clone(),
            text: replacement,
            cursor,
        })
    }
}

#[cfg(test)]
mod tests;
