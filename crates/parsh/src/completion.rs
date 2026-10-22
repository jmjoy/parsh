//! Completion for unfinished source, without evaluating any of it.
use std::ops::Range;

use crate::{reader, shell::Shell};

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
    quoted: bool,
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
        quoted: bool,
        closed: bool,
    ) -> Option<Self> {
        let frame = frames.last()?;
        if frames.iter().any(|frame| frame.comment) || frame.declaration() {
            return None;
        }
        let kind = if !quoted && prefix.starts_with('$') {
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
            quoted,
            closed,
        })
    }

    fn parse(source: &str, cursor: usize) -> Option<Self> {
        if !source.is_char_boundary(cursor) {
            return None;
        }
        let mut frames: Vec<Frame> = Vec::new();
        let mut offset = 0;
        if source.starts_with("#!") {
            offset = source.find('\n').unwrap_or(source.len());
            if cursor < offset {
                return None;
            }
        }
        while offset < source.len() && offset <= cursor {
            let ch = source[offset..].chars().next()?;
            if offset == cursor && (ch.is_whitespace() || matches!(ch, '(' | ')')) {
                return Self::at(&frames, cursor..cursor, String::new(), false, false);
            }
            match ch {
                ch if ch.is_whitespace() => offset += ch.len_utf8(),
                '(' => {
                    let parameters = frames.last().is_some_and(|parent| {
                        parent.parameters
                            || (parent.head.as_deref() == Some("defn") && parent.items == 2)
                    });
                    frames.push(Frame {
                        parameters,
                        ..Frame::default()
                    });
                    offset += 1;
                }
                ')' => {
                    let frame = frames.pop()?;
                    if !frame.comment
                        && let Some(parent) = frames.last_mut()
                    {
                        parent.items += 1;
                    }
                    offset += 1;
                }
                '"' => {
                    let start = offset;
                    offset += 1;
                    let mut escaped = false;
                    let mut closed = false;
                    while offset < source.len() {
                        let ch = source[offset..].chars().next()?;
                        offset += ch.len_utf8();
                        if escaped {
                            escaped = false;
                        } else if ch == '\\' {
                            escaped = true;
                        } else if ch == '"' {
                            closed = true;
                            break;
                        }
                    }
                    let content_end = offset - usize::from(closed);
                    if cursor <= offset {
                        let end = cursor.max(start + 1).min(content_end);
                        let prefix = unescape(&source[start + 1..end])?;
                        return Self::at(&frames, start..offset, prefix, true, closed);
                    }
                    frames
                        .last_mut()?
                        .word(unescape(&source[start + 1..content_end])?, true);
                }
                _ => {
                    let start = offset;
                    offset += ch.len_utf8();
                    while offset < source.len() {
                        let ch = source[offset..].chars().next()?;
                        if !reader::word_character(ch) {
                            break;
                        }
                        offset += ch.len_utf8();
                    }
                    let text = &source[start..offset];
                    if cursor <= offset {
                        if text == ";" && frames.last()?.items == 0 {
                            return None;
                        }
                        return Self::at(
                            &frames,
                            start..offset,
                            source[start..cursor].into(),
                            false,
                            false,
                        );
                    }
                    frames.last_mut()?.word(text.into(), false);
                }
            }
        }
        (offset == cursor)
            .then(|| Self::at(&frames, cursor..cursor, String::new(), false, false))
            .flatten()
    }
}

fn unescape(text: &str) -> Option<String> {
    let mut decoded = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        decoded.push(if ch == '\\' {
            match chars.next()? {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                _ => return None,
            }
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
        let finished = unique && !first.directory;
        let quoted =
            self.context.kind != Kind::Variable && (self.context.quoted || needs_quotes(text));
        let mut replacement = if quoted {
            format!("\"{}", quoted_content(text))
        } else {
            text.into()
        };
        let mut cursor = replacement.len();
        if quoted && (finished || self.context.closed) {
            replacement.push('"');
            if finished {
                cursor += 1;
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
