//! Source-preserving structural editing, including unfinished pasted source.
use super::{Input, Key};
use crate::reader::{self, Quote};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Atom,
    Group(char),
    String(Quote),
}

struct Form {
    range: Range<usize>,
    open: Range<usize>,
    close: Option<Range<usize>>,
    shape: Shape,
    parent: Option<usize>,
    children: Vec<usize>,
    complete: bool,
    implicit: bool,
    interpolation: bool,
}

impl Form {
    fn content_end(&self) -> usize {
        self.close.as_ref().map_or(self.range.end, |r| r.start)
    }

    fn opening(&self) -> Range<usize> {
        self.range.start..self.open.end
    }

    fn interior(&self, cursor: usize) -> bool {
        self.shape != Shape::Atom && self.open.end <= cursor && cursor <= self.content_end()
    }
}

pub(super) struct Syntax {
    source: String,
    shift: usize,
    forms: Vec<Form>,
    escapes: Vec<Range<usize>>,
}

struct Scanner {
    syntax: Syntax,
    offset: usize,
    quasiquote: usize,
}

impl Scanner {
    fn peek(&self) -> Option<char> {
        self.syntax.source[self.offset..].chars().next()
    }

    fn bump(&mut self) {
        if let Some(ch) = self.peek() {
            self.offset += ch.len_utf8();
        }
    }

    fn space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn add(&mut self, shape: Shape, parent: Option<usize>, start: usize) -> usize {
        let id = self.syntax.forms.len();
        self.syntax.forms.push(Form {
            range: start..self.offset,
            open: start..self.offset,
            close: None,
            shape,
            parent,
            children: Vec::new(),
            complete: false,
            implicit: false,
            interpolation: false,
        });
        if let Some(parent) = parent {
            self.syntax.forms[parent].children.push(id);
        }
        id
    }

    fn form(&mut self, parent: Option<usize>, depth: usize) -> usize {
        let start = self.offset;
        let ch = self.peek().unwrap();
        if depth >= 256 {
            self.bump();
            return self.add(Shape::Atom, parent, start);
        }
        if ch == '`' || (ch == ',' && self.quasiquote > 0) {
            self.bump();
            if ch == ',' && self.peek() == Some('@') {
                self.bump();
            }
            let saved = self.quasiquote;
            self.quasiquote += usize::from(ch == '`');
            let id = if self
                .peek()
                .is_some_and(|c| !c.is_whitespace() && !matches!(c, ')' | ']' | '}'))
            {
                self.form(parent, depth + 1)
            } else {
                self.add(Shape::Atom, parent, start)
            };
            self.syntax.forms[id].range.start = start;
            self.quasiquote = saved;
            return id;
        }
        match ch {
            '(' | '[' | '{' => self.group(parent, depth, start, false),
            '\'' | '"' => self.string(parent, depth),
            _ => {
                self.bump();
                while self.peek().is_some_and(|ch| {
                    reader::word_character(ch) && !(self.quasiquote > 0 && ch == ',')
                }) {
                    self.bump();
                }
                let id = self.add(Shape::Atom, parent, start);
                self.syntax.forms[id].complete = !matches!(ch, ')' | ']' | '}');
                id
            }
        }
    }

    fn group(
        &mut self,
        parent: Option<usize>,
        depth: usize,
        start: usize,
        interpolation: bool,
    ) -> usize {
        let open_start = self.offset;
        let open = self.peek().unwrap();
        let close = reader::closing_delimiter(open).unwrap();
        self.bump();
        let id = self.add(Shape::Group(open), parent, start);
        self.syntax.forms[id].open = open_start..self.offset;
        self.syntax.forms[id].implicit = self.syntax.shift == 1 && open_start == 0;
        self.syntax.forms[id].interpolation = interpolation;
        let saved = self.quasiquote;
        let mut valid = true;
        loop {
            self.space();
            match self.peek() {
                None => break,
                Some(ch) if ch == close => {
                    let start = self.offset;
                    self.bump();
                    self.syntax.forms[id].close = Some(start..self.offset);
                    break;
                }
                Some(ch @ (')' | ']' | '}')) => {
                    // Leave an ancestor's closer available when a pasted child is unfinished.
                    let mut ancestor = parent;
                    let mut inherited = false;
                    while let Some(index) = ancestor {
                        let form = &self.syntax.forms[index];
                        if matches!(form.shape, Shape::Group(open) if reader::closing_delimiter(open) == Some(ch))
                        {
                            inherited = true;
                            break;
                        }
                        ancestor = form.parent;
                    }
                    if inherited {
                        break;
                    }
                    self.form(Some(id), depth + 1);
                    valid = false;
                }
                _ => {
                    let child = self.form(Some(id), depth + 1);
                    valid &= self.syntax.forms[child].complete;
                    if self.syntax.forms[id].children.len() == 1
                        && open == '('
                        && &self.syntax.source[self.syntax.forms[child].range.clone()]
                            == "quasiquote"
                    {
                        self.quasiquote += 1;
                    }
                }
            }
        }
        self.quasiquote = saved;
        let form = &mut self.syntax.forms[id];
        form.range.end = self.offset;
        form.complete = valid && (form.close.is_some() || form.implicit);
        id
    }

    fn string(&mut self, parent: Option<usize>, depth: usize) -> usize {
        let start = self.offset;
        let quote = Quote::at(&self.syntax.source[start..]);
        let delimiter = quote.delimiter();
        self.offset += delimiter.len();
        let id = self.add(Shape::String(quote), parent, start);
        let mut valid = true;
        while let Some(ch) = self.peek() {
            if self.syntax.source[self.offset..].starts_with(delimiter) {
                let start = self.offset;
                self.offset += delimiter.len();
                self.syntax.forms[id].close = Some(start..self.offset);
                break;
            }
            if quote.interpolates() && ch == '\\' {
                let start = self.offset;
                self.bump();
                valid &= self.peek().and_then(reader::string_escape).is_some();
                self.bump();
                self.syntax.escapes.push(start..self.offset);
            } else if quote.interpolates()
                && ch == '$'
                && self.syntax.source[self.offset..].starts_with("$(")
                && depth + 1 < 256
            {
                let start = self.offset;
                self.bump();
                let child = self.group(Some(id), depth + 1, start, true);
                valid &= self.syntax.forms[child].complete;
            } else {
                self.bump();
            }
        }
        let form = &mut self.syntax.forms[id];
        form.range.end = self.offset;
        form.complete = valid && form.close.is_some();
        id
    }
}

fn intersects(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

impl Syntax {
    pub(super) fn new(input: &Input) -> Self {
        Self::scan(input.source(), usize::from(input.implicit()))
    }

    fn scan(source: String, shift: usize) -> Self {
        let mut scanner = Scanner {
            syntax: Self {
                source,
                shift,
                forms: Vec::new(),
                escapes: Vec::new(),
            },
            offset: 0,
            quasiquote: 0,
        };
        if scanner.syntax.source.starts_with("#!") {
            scanner.offset = scanner
                .syntax
                .source
                .find('\n')
                .unwrap_or(scanner.syntax.source.len());
        }
        loop {
            scanner.space();
            if scanner.peek().is_none() {
                break;
            }
            scanner.form(None, 0);
        }
        scanner.syntax
    }

    fn complete(&self) -> bool {
        self.forms.iter().all(|form| form.complete)
    }

    fn accepts(&self, source: String) -> bool {
        !self.complete() || Self::scan(source, self.shift).complete()
    }

    pub(super) fn accepts_edit(&self, range: Range<usize>, text: &str) -> bool {
        let mut source = self.source.clone();
        source.replace_range(range.start + self.shift..range.end + self.shift, text);
        self.accepts(source)
    }

    fn body_position(&self, offset: usize) -> usize {
        offset.saturating_sub(self.shift)
    }

    fn container(&self, cursor: usize) -> Option<usize> {
        self.forms.iter().rposition(|form| form.interior(cursor))
    }

    fn string_at(&self, cursor: usize) -> Option<usize> {
        self.forms.iter().enumerate().rev().find_map(|(id, form)| {
            if !matches!(form.shape, Shape::String(_))
                || cursor <= form.open.start
                || (cursor >= form.range.end && form.close.is_some())
                || cursor > form.range.end
            {
                return None;
            }
            if form
                .children
                .iter()
                .any(|&child| self.forms[child].interior(cursor))
            {
                return None;
            }
            Some(id)
        })
    }

    fn next(&self, cursor: usize) -> Option<usize> {
        if let Some(id) = self.string_at(cursor) {
            return Some(id);
        }
        if let Some(id) = self.forms.iter().position(|form| {
            !form.implicit
                && form.range.start <= cursor
                && cursor < form.range.end
                && (form.shape == Shape::Atom || cursor < form.open.end)
        }) {
            return Some(id);
        }
        let parent = self.container(cursor);
        self.forms
            .iter()
            .position(|form| form.parent == parent && form.range.start >= cursor)
    }

    fn previous(&self, cursor: usize) -> Option<usize> {
        if let Some(id) = self.string_at(cursor) {
            return Some(id);
        }
        if let Some(id) = self.forms.iter().position(|form| {
            !form.implicit
                && form.range.start < cursor
                && cursor <= form.range.end
                && (form.shape == Shape::Atom || cursor > form.content_end())
        }) {
            return Some(id);
        }
        let parent = self.container(cursor);
        self.forms
            .iter()
            .rposition(|form| form.parent == parent && form.range.end <= cursor)
    }

    pub(super) fn safe_range(&self, range: Range<usize>, backward: bool) -> Range<usize> {
        let mut range = range.start + self.shift..range.end + self.shift;
        loop {
            let before = range.clone();
            for form in &self.forms {
                let Some(close) = &form.close else { continue };
                if form.implicit || (range.start <= form.range.start && range.end >= form.range.end)
                {
                    continue;
                }
                for boundary in [form.opening(), close.clone()] {
                    Self::trim(&mut range, &boundary, backward);
                }
            }
            for escape in &self.escapes {
                if !(range.start <= escape.start && range.end >= escape.end) {
                    Self::trim(&mut range, escape, backward);
                }
            }
            if before == range {
                break;
            }
        }
        // Removing text next to a triple delimiter can turn a short quote run into
        // a different delimiter. Preserve a boundary character in that case.
        while !range.is_empty() {
            let mut source = self.source.clone();
            source.replace_range(range.clone(), "");
            if self.accepts(source) {
                break;
            }
            if backward {
                range.start += self.source[range.clone()]
                    .graphemes(true)
                    .next()
                    .unwrap()
                    .len();
            } else {
                range.end = self.source[..range.end]
                    .grapheme_indices(true)
                    .next_back()
                    .unwrap()
                    .0;
            }
        }
        self.body_position(range.start)..self.body_position(range.end)
    }

    fn trim(range: &mut Range<usize>, boundary: &Range<usize>, backward: bool) {
        if intersects(range, boundary) {
            if backward {
                range.start = range.start.max(boundary.end.min(range.end));
            } else {
                range.end = range.end.min(boundary.start.max(range.start));
            }
        }
    }

    pub(super) fn plain(&self, range: Range<usize>) -> bool {
        let range = range.start + self.shift..range.end + self.shift;
        self.forms.iter().any(|form| {
            matches!(form.shape, Shape::Atom | Shape::String(_))
                && form.range.start <= range.start
                && range.end <= form.range.end
        }) && !self.forms.iter().any(|form| {
            form.shape != Shape::Atom
                && (intersects(&range, &form.opening())
                    || form
                        .close
                        .as_ref()
                        .is_some_and(|close| intersects(&range, close)))
        }) && !self.escapes.iter().any(|escape| intersects(&range, escape))
    }

    fn seam(&self, range: &Range<usize>) -> String {
        if self.source[..range.start]
            .chars()
            .next_back()
            .is_some_and(reader::word_character)
            && self.source[range.end..]
                .chars()
                .next()
                .is_some_and(reader::word_character)
        {
            " ".into()
        } else {
            String::new()
        }
    }

    fn apply(&self, input: &mut Input, mut edits: Vec<(Range<usize>, String)>) -> bool {
        edits.sort_by_key(|(range, _)| range.start);
        let mut source = self.source.clone();
        for (range, text) in edits.iter().rev() {
            source.replace_range(range.clone(), text);
        }
        if !self.accepts(source) {
            return false;
        }
        for (range, text) in edits.into_iter().rev() {
            let range = self.body_position(range.start)..self.body_position(range.end);
            if range.is_empty() && input.cursor == range.start {
                // Keep point before a delimiter inserted exactly at point.
            } else if input.cursor >= range.end {
                input.cursor = input.cursor - range.len() + text.len();
            } else if input.cursor > range.start {
                input.cursor = range.start + text.len();
            }
            input.body.replace_range(range, &text);
        }
        if self.shift == 0
            && !input.explicit
            && input
                .body
                .trim_start()
                .chars()
                .next()
                .is_some_and(|ch| reader::closing_delimiter(ch).is_some())
        {
            input.explicit = true;
        }
        true
    }

    pub(super) fn delete(&self, input: &mut Input, backward: bool) {
        let cursor = input.cursor + self.shift;
        let range = if backward {
            self.source[..cursor]
                .grapheme_indices(true)
                .next_back()
                .map(|(start, _)| start..cursor)
        } else {
            self.source[cursor..]
                .graphemes(true)
                .next()
                .map(|g| cursor..cursor + g.len())
        };
        let Some(range) = range else { return };
        if range.start < self.shift {
            return;
        }
        for form in self.forms.iter().rev() {
            let Some(close) = &form.close else { continue };
            let opening = form.opening();
            let empty = self.source[form.open.end..close.start].is_empty()
                || (matches!(form.shape, Shape::Group(_))
                    && self.source[form.open.end..close.start].trim().is_empty());
            if !form.implicit && empty && (form.open.end..=close.start).contains(&cursor) {
                self.apply(input, vec![(form.range.clone(), self.seam(&form.range))]);
                return;
            }
            for boundary in [opening, close.clone()] {
                if intersects(&range, &boundary) {
                    input.cursor = self.body_position(if backward {
                        boundary.start
                    } else {
                        boundary.end
                    });
                    return;
                }
            }
        }
        if let Some(escape) = self
            .escapes
            .iter()
            .find(|escape| intersects(&range, escape))
        {
            self.apply(input, vec![(escape.clone(), String::new())]);
        } else if self.accepts_edit(
            self.body_position(range.start)..self.body_position(range.end),
            "",
        ) {
            if backward {
                input.backspace();
            } else {
                input.delete();
            }
        }
    }

    pub(super) fn kill_sexp(&self, input: &Input) -> Range<usize> {
        let cursor = input.cursor + self.shift;
        let Some(id) = self.next(cursor) else {
            return input.cursor..input.cursor;
        };
        let form = &self.forms[id];
        if !form.complete {
            return input.cursor..input.cursor;
        }
        self.safe_range(input.cursor..self.body_position(form.range.end), false)
    }

    pub(super) fn command(&self, input: &mut Input, key: Key) {
        let cursor = input.cursor + self.shift;
        match key {
            Key::SexpForward | Key::SexpBackward => {
                let backward = matches!(key, Key::SexpBackward);
                let target = if backward {
                    self.previous(cursor)
                } else {
                    self.next(cursor)
                };
                let offset = target
                    .map(|id| {
                        let form = &self.forms[id];
                        if backward {
                            form.range.start
                        } else {
                            form.range.end
                        }
                    })
                    .or_else(|| {
                        let form = &self.forms[self.container(cursor)?];
                        if form.implicit {
                            return None;
                        }
                        Some(if backward {
                            form.range.start
                        } else {
                            form.range.end
                        })
                    });
                if let Some(offset) = offset {
                    input.cursor = self.body_position(offset);
                }
            }
            Key::SexpUp => {
                if let Some(id) = self.container(cursor) {
                    let form = &self.forms[id];
                    if !form.implicit {
                        input.cursor = self.body_position(form.range.start);
                    }
                }
            }
            Key::SexpDown => {
                let parent = self.container(cursor);
                if let Some(form) = self.forms.iter().find(|form| {
                    form.parent == parent
                        && matches!(form.shape, Shape::Group(_))
                        && form.range.start >= cursor
                }) {
                    input.cursor = self.body_position(form.open.end);
                }
            }
            Key::WrapSexp => {
                let range = if let Some(id) = self.next(cursor) {
                    let form = &self.forms[id];
                    if !form.complete {
                        return;
                    }
                    form.range.clone()
                } else {
                    cursor..cursor
                };
                let start = self.body_position(range.start);
                if range.is_empty() {
                    input.insert('(');
                    input.body.insert(input.cursor, ')');
                } else {
                    if self.apply(
                        input,
                        vec![
                            (range.end..range.end, ")".into()),
                            (range.start..range.start, "(".into()),
                        ],
                    ) {
                        input.cursor = input.cursor.max(start + 1);
                    }
                }
            }
            Key::SpliceSexp => {
                let Some(id) = self.container(cursor) else {
                    return;
                };
                let form = &self.forms[id];
                if !matches!(form.shape, Shape::Group(_))
                    || form.implicit
                    || form.interpolation
                    || !form.complete
                {
                    return;
                }
                let close = form.close.clone().unwrap();
                if form.open.end == close.start {
                    self.apply(input, vec![(form.range.clone(), self.seam(&form.range))]);
                } else {
                    let open = form.opening();
                    self.apply(
                        input,
                        vec![
                            (open.clone(), self.seam(&open)),
                            (close.clone(), self.seam(&close)),
                        ],
                    );
                }
            }
            Key::SlurpForward | Key::SlurpBackward | Key::BarfForward | Key::BarfBackward => {
                self.reshape(input, key, cursor);
            }
            _ => unreachable!(),
        }
    }

    fn reshape(&self, input: &mut Input, key: Key, cursor: usize) {
        let slurp = matches!(key, Key::SlurpForward | Key::SlurpBackward);
        let backward = matches!(key, Key::SlurpBackward | Key::BarfBackward);
        let mut current = self.container(cursor);
        while let Some(id) = current {
            let form = &self.forms[id];
            if !matches!(form.shape, Shape::Group(_))
                || form.implicit
                || form.interpolation
                || !form.complete
            {
                return;
            }
            let open = form.opening();
            let close = form.close.clone().unwrap();
            let moved = if backward { open } else { close };
            let target = if slurp {
                let sibling = if backward {
                    self.forms.iter().rposition(|sibling| {
                        sibling.parent == form.parent && sibling.range.end <= form.range.start
                    })
                } else {
                    self.forms.iter().position(|sibling| {
                        sibling.parent == form.parent && sibling.range.start >= form.range.end
                    })
                };
                let Some(sibling) = sibling else {
                    current = form.parent;
                    continue;
                };
                let sibling = &self.forms[sibling];
                if !sibling.complete {
                    return;
                }
                if backward {
                    sibling.range.start
                } else {
                    sibling.range.end
                }
            } else if form.children.is_empty() {
                return;
            } else if backward {
                form.children
                    .get(1)
                    .map_or(form.content_end(), |&id| self.forms[id].range.start)
            } else {
                form.children
                    .iter()
                    .rev()
                    .nth(1)
                    .map_or(form.open.end, |&id| self.forms[id].range.end)
            };
            let delimiter = self.source[moved.clone()].to_owned();
            let advance = backward && target == cursor;
            let length = delimiter.len();
            let applied = self.apply(
                input,
                vec![
                    (moved.clone(), self.seam(&moved)),
                    (target..target, delimiter),
                ],
            );
            if applied && advance {
                input.cursor += length;
            }
            return;
        }
    }
}

#[derive(Default)]
pub(super) struct Paredit {
    quotes: Option<(char, usize, u8)>,
    escape: Option<usize>,
}

impl Paredit {
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn insert(&mut self, input: &mut Input, ch: char) {
        let before = input.clone();
        let complete = Syntax::new(input).complete();
        self.insert_character(input, ch);
        if complete && !matches!(ch, '`' | ',') && !Syntax::new(input).complete() {
            *input = before;
            self.clear();
        }
    }

    fn insert_character(&mut self, input: &mut Input, ch: char) {
        if let Some(cursor) = self.escape.take()
            && input.cursor == cursor
            && input.body[cursor..].starts_with('\\')
        {
            input
                .body
                .replace_range(cursor..cursor + 1, &ch.to_string());
            input.cursor += ch.len_utf8();
            return;
        }
        if let Some((quote, start, count)) = self.quotes.take()
            && ch == quote
            && input.cursor == start + usize::from(count)
            && input.body[start..].starts_with(&quote.to_string().repeat(2))
        {
            if count == 1 {
                input.cursor += 1;
                self.quotes = Some((quote, start, 2));
            } else {
                input
                    .body
                    .replace_range(start..start + 2, &quote.to_string().repeat(6));
                input.cursor = start + 3;
            }
            return;
        }
        let syntax = Syntax::new(input);
        if let Some(escape) = syntax.escapes.iter().find(|escape| {
            escape.start < input.cursor + syntax.shift && input.cursor + syntax.shift < escape.end
        }) {
            input.cursor = syntax.body_position(escape.end);
        }
        let cursor = input.cursor + syntax.shift;
        if let Some(id) = syntax.string_at(cursor) {
            let form = &syntax.forms[id];
            let Shape::String(quote) = form.shape else {
                unreachable!()
            };
            if form
                .close
                .as_ref()
                .is_some_and(|close| close.contains(&cursor))
                && quote.delimiter().starts_with(ch)
            {
                input.cursor += ch.len_utf8();
            } else if ch == '\\' && quote.interpolates() {
                input.insert('\\');
                input.body.insert(input.cursor, '\\');
                self.escape = Some(input.cursor);
            } else if ch == '('
                && quote.interpolates()
                && syntax.source[..cursor].ends_with('$')
                && !syntax.escapes.iter().any(|escape| escape.end == cursor)
            {
                input.insert('(');
                input.body.insert(input.cursor, ')');
            } else if ch == '"' && quote == Quote::Double && form.close.is_some() {
                input.insert('\\');
                input.insert('"');
            } else if ch == '\'' && quote == Quote::Single && form.close.is_some() {
                input.cursor = syntax.body_position(form.range.end);
            } else if quote.delimiter().len() == 3
                && form.close.is_some()
                && quote.delimiter().starts_with(ch)
                && syntax.source[..cursor].ends_with(&ch.to_string().repeat(2))
            {
                if quote.interpolates() {
                    input.insert('\\');
                    input.insert(ch);
                }
            } else {
                input.insert(ch);
            }
            return;
        }
        if let Some(close) = reader::closing_delimiter(ch) {
            input.insert(ch);
            input.body.insert(input.cursor, close);
        } else if matches!(ch, ')' | ']' | '}') {
            if let Some(id) = syntax.container(cursor) {
                let form = &syntax.forms[id];
                if matches!(form.shape, Shape::Group(open) if reader::closing_delimiter(open) == Some(ch))
                {
                    if let Some(close) = &form.close {
                        input.cursor = syntax.body_position(close.end);
                    } else if form.implicit {
                        input.cursor = input.body.len();
                    } else {
                        input.insert(ch);
                    }
                }
            }
        } else if matches!(ch, '\'' | '"') {
            if syntax.forms.iter().any(|form| {
                matches!(form.shape, Shape::String(quote) if quote.delimiter().len() == 1 && quote.delimiter().starts_with(ch))
                    && form.range.end == cursor
                    && form.open.end == form.content_end()
            }) {
                input.insert(' ');
            }
            let start = input.cursor;
            input.insert(ch);
            input.body.insert(input.cursor, ch);
            if input.body[input.cursor + 1..].starts_with(ch) {
                input.body.insert(input.cursor + 1, ' ');
            }
            self.quotes = Some((ch, start, 1));
        } else {
            input.insert(ch);
        }
    }
}
