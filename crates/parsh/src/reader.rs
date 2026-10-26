use std::fmt;

use crate::number::Number;

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: Kind,
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Word(String),
    Text(String),
    Number(Number),
    Bool(bool),
    Nil,
    Variable(String),
    /// Double-quoted text fragments and embedded variable or call expressions.
    Interpolated(Vec<Expr>),
    List(Vec<Expr>),
    Comment,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub offset: usize,
    pub message: String,
}

impl Expr {
    pub(crate) fn is_top_level(&self) -> bool {
        matches!(
            self.kind,
            Kind::List(_) | Kind::Comment | Kind::Number(_) | Kind::Bool(_) | Kind::Nil
        )
    }

    /// Render an expression back to source. Quoted forms are printed with it.
    pub fn source(&self) -> String {
        match &self.kind {
            Kind::Word(text) => text.clone(),
            Kind::Text(text) => format!("\"{}\"", escape_text(text)),
            Kind::Number(number) => format!("#{number}"),
            Kind::Bool(value) => format!("#{value}"),
            Kind::Nil => "#nil".into(),
            Kind::Variable(name) => format!("${name}"),
            Kind::Interpolated(parts) => {
                let mut rendered = String::from("\"");
                for part in parts {
                    match &part.kind {
                        Kind::Variable(name) => rendered.push_str(&format!("${name}")),
                        Kind::Text(text) => rendered.push_str(&escape_text(text)),
                        // `$( ... )` parts are always lists, so the source already has parens.
                        _ => rendered.push_str(&format!("${}", part.source())),
                    }
                }
                rendered.push('"');
                rendered
            }
            Kind::List(items) => {
                let items: Vec<_> = items.iter().map(Expr::source).collect();
                format!("({})", items.join(" "))
            }
            Kind::Comment => "(;)".into(),
        }
    }
}

/// Escape text for a double-quoted string the reader can read back.
fn escape_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '$' => escaped.push_str("\\$"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch => escaped.push(ch),
        }
    }
    escaped
}

impl Diagnostic {
    pub fn render(&self, name: &str, source: &str) -> String {
        let (line, column) = position(source, self.offset);
        format!("{name}:{line}:{column}: {}", self.message)
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

pub fn position(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
    let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, column)
}

#[derive(Debug, PartialEq)]
pub enum Read {
    Form(Expr),
    End,
    Incomplete(Diagnostic),
    ExtraClose(Diagnostic),
    Error(Diagnostic),
}

/// String boundaries shared by the reader and completion scanner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Quote {
    #[default]
    None,
    Single,
    Double,
    TripleSingle,
    TripleDouble,
}

impl Quote {
    pub(crate) fn at(source: &str) -> Self {
        if source.starts_with("'''") {
            Self::TripleSingle
        } else if source.starts_with("\"\"\"") {
            Self::TripleDouble
        } else if source.starts_with('\'') {
            Self::Single
        } else if source.starts_with('"') {
            Self::Double
        } else {
            Self::None
        }
    }

    pub(crate) fn delimiter(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Single => "'",
            Self::Double => "\"",
            Self::TripleSingle => "'''",
            Self::TripleDouble => "\"\"\"",
        }
    }

    pub(crate) fn interpolates(self) -> bool {
        matches!(self, Self::Double | Self::TripleDouble)
    }
}

/// An incremental reader. An unexpected top-level closing delimiter is always consumed.
pub struct Reader<'a> {
    source: &'a str,
    pub offset: usize,
    /// Nonzero while reading a quasiquote template, where `,` starts an unquote.
    quasiquote: usize,
    /// Comments still balance strings and delimiters, but do not parse literals.
    comments: usize,
}

impl<'a> Reader<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            offset: 0,
            quasiquote: 0,
            comments: 0,
        }
    }

    pub fn read(&mut self) -> Read {
        self.space();
        if self.offset == self.source.len() {
            return Read::End;
        }
        if let Some(close @ (')' | ']' | '}')) = self.peek() {
            let offset = self.offset;
            self.bump();
            return Read::ExtraClose(Diagnostic {
                offset,
                message: format!("unexpected '{close}'"),
            });
        }
        let result = self.expr(0);
        self.quasiquote = 0;
        self.comments = 0;
        match result {
            Ok(expr) => Read::Form(expr),
            Err((incomplete, diagnostic)) => {
                if incomplete {
                    Read::Incomplete(diagnostic)
                } else {
                    Read::Error(diagnostic)
                }
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    fn space(&mut self) {
        if self.offset == 0 && self.source.starts_with("#!") {
            self.offset = self.source.find('\n').unwrap_or(self.source.len());
        }
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn error(&self, offset: usize, message: &str, incomplete: bool) -> (bool, Diagnostic) {
        (
            incomplete,
            Diagnostic {
                offset,
                message: message.into(),
            },
        )
    }

    fn expr(&mut self, depth: usize) -> Result<Expr, (bool, Diagnostic)> {
        if depth >= 256 {
            return Err(self.error(self.offset, "expression nesting exceeds 256 levels", false));
        }
        let offset = self.offset;
        let kind = match self.bump() {
            Some(open @ ('(' | '[' | '{')) => {
                let close = closing_delimiter(open).unwrap();
                let mut items = Vec::new();
                if let Some(constructor) = collection_constructor(open) {
                    items.push(Expr {
                        kind: Kind::Word(constructor.into()),
                        offset,
                    });
                }
                let mut quasiquote = false;
                let mut comment = false;
                loop {
                    self.space();
                    match self.peek() {
                        None => {
                            return Err(self.error(offset, &format!("unclosed '{open}'"), true));
                        }
                        Some(ch) if ch == close => {
                            self.bump();
                            break;
                        }
                        Some(ch @ (')' | ']' | '}')) => {
                            return Err(self.error(
                                self.offset,
                                &format!("unexpected '{ch}', expected '{close}'"),
                                false,
                            ));
                        }
                        _ => {
                            let item = self.expr(depth + 1)?;
                            if item.kind != Kind::Comment {
                                if items.is_empty()
                                    && open == '('
                                    && matches!(&item.kind, Kind::Word(head) if head == ";")
                                {
                                    comment = true;
                                    self.comments += 1;
                                }
                                // `(quasiquote template)` enables `,` just like a backquote.
                                if !quasiquote
                                    && items.is_empty()
                                    && open == '('
                                    && matches!(&item.kind, Kind::Word(head) if head == "quasiquote")
                                {
                                    quasiquote = true;
                                    self.quasiquote += 1;
                                }
                                items.push(item);
                            }
                        }
                    }
                }
                if quasiquote {
                    self.quasiquote -= 1;
                }
                if comment {
                    self.comments -= 1;
                }
                if open == '('
                    && matches!(items.first().map(|e| &e.kind), Some(Kind::Word(s)) if s == ";")
                {
                    Kind::Comment
                } else {
                    Kind::List(items)
                }
            }
            Some('`') => {
                self.quasiquote += 1;
                let inner = self.expr(depth + 1);
                self.quasiquote -= 1;
                Kind::List(vec![
                    Expr {
                        kind: Kind::Word("quasiquote".into()),
                        offset,
                    },
                    inner?,
                ])
            }
            Some(',') if self.quasiquote > 0 => {
                let splicing = self.peek() == Some('@');
                if splicing {
                    self.bump();
                }
                if let Some(close @ (')' | ']' | '}')) = self.peek() {
                    return Err(self.error(
                        offset,
                        &format!("expected an expression after ',', found '{close}'"),
                        false,
                    ));
                }
                let inner = self.expr(depth + 1)?;
                Kind::List(vec![
                    Expr {
                        kind: Kind::Word(
                            if splicing {
                                "unquote-splicing"
                            } else {
                                "unquote"
                            }
                            .into(),
                        ),
                        offset,
                    },
                    inner,
                ])
            }
            Some('\'' | '"') => {
                let quote = Quote::at(&self.source[offset..]);
                self.offset = offset + quote.delimiter().len();
                self.string(offset, depth, quote)?
            }
            Some(ch) => {
                let mut text = String::from(ch);
                while self
                    .peek()
                    .is_some_and(|ch| word_character(ch) && !(self.quasiquote > 0 && ch == ','))
                {
                    text.push(self.bump().unwrap());
                }
                if self.comments == 0 && text.starts_with('#') {
                    match &text[1..] {
                        "true" => Kind::Bool(true),
                        "false" => Kind::Bool(false),
                        "nil" => Kind::Nil,
                        literal => Kind::Number(Number::parse(literal).map_err(|message| {
                            self.error(
                                offset,
                                &format!("invalid literal {text:?}: {message}"),
                                false,
                            )
                        })?),
                    }
                } else if let Some(name) = text.strip_prefix('$') {
                    Kind::Variable(name.into())
                } else {
                    Kind::Word(text)
                }
            }
            None => return Err(self.error(offset, "expected an expression", true)),
        };
        Ok(Expr { kind, offset })
    }

    fn string(
        &mut self,
        offset: usize,
        depth: usize,
        quote: Quote,
    ) -> Result<Kind, (bool, Diagnostic)> {
        let mut parts = Vec::new();
        let mut text = String::new();
        let mut text_offset = self.offset;
        loop {
            if self.source[self.offset..].starts_with(quote.delimiter()) {
                self.offset += quote.delimiter().len();
                break;
            }
            match self.bump() {
                None => {
                    let message = match quote {
                        Quote::Single => "unclosed single-quoted string",
                        Quote::TripleSingle => "unclosed triple-single-quoted string",
                        Quote::TripleDouble => "unclosed triple-double-quoted string",
                        _ => "unclosed string",
                    };
                    return Err(self.error(offset, message, true));
                }
                Some('\\') if quote.interpolates() => {
                    let escape_at = self.offset - 1;
                    let Some(ch) = self.bump() else {
                        return Err(self.error(offset, "unclosed string escape", true));
                    };
                    text.push(
                        string_escape(ch)
                            .ok_or_else(|| self.error(escape_at, "unknown string escape", false))?,
                    );
                }
                Some('$') if quote.interpolates() => {
                    let variable_offset = self.offset - 1;
                    let part = match self.peek() {
                        Some('(') => self.expr(depth + 1)?,
                        Some('?') => {
                            self.bump();
                            Expr {
                                kind: Kind::Variable("?".into()),
                                offset: variable_offset,
                            }
                        }
                        Some(ch) if variable_start(ch) => {
                            let start = self.offset;
                            while self.peek().is_some_and(variable_continue) {
                                self.bump();
                            }
                            Expr {
                                kind: Kind::Variable(self.source[start..self.offset].into()),
                                offset: variable_offset,
                            }
                        }
                        _ => {
                            text.push('$');
                            continue;
                        }
                    };
                    if !text.is_empty() {
                        parts.push(Expr {
                            kind: Kind::Text(std::mem::take(&mut text)),
                            offset: text_offset,
                        });
                    }
                    parts.push(part);
                    text_offset = self.offset;
                }
                Some(ch) => text.push(ch),
            }
        }
        if parts.is_empty() {
            Ok(Kind::Text(text))
        } else {
            if !text.is_empty() {
                parts.push(Expr {
                    kind: Kind::Text(text),
                    offset: text_offset,
                });
            }
            Ok(Kind::Interpolated(parts))
        }
    }
}

/// Bare words have the same boundaries in the reader and completion scanner.
pub(crate) fn word_character(ch: char) -> bool {
    !ch.is_whitespace() && !matches!(ch, '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'')
}

pub(crate) fn closing_delimiter(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

pub(crate) fn collection_constructor(open: char) -> Option<&'static str> {
    match open {
        '[' => Some("list"),
        '{' => Some("map"),
        _ => None,
    }
}

pub(crate) fn variable_start(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic()
}

pub(crate) fn variable_continue(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

pub(crate) fn variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(variable_start) && chars.all(variable_continue)
}

pub(crate) fn string_escape(ch: char) -> Option<char> {
    match ch {
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        '\\' | '"' | '$' => Some(ch),
        _ => None,
    }
}

/// True only for one complete top-level expression and no tail.
pub fn complete(source: &str) -> bool {
    let mut reader = Reader::new(source);
    matches!(
        reader.read(),
        Read::Form(expr) if expr.is_top_level()
    ) && reader.read() == Read::End
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_sugar_preserves_offsets_and_delimits_adjacent_expressions() {
        let source = " [甲{key $value}(+ 1 2)(; ignored)][]{}tail";
        let mut reader = Reader::new(source);
        let Read::Form(expr) = reader.read() else {
            panic!()
        };
        assert_eq!(expr.offset, 1);
        let Kind::List(items) = expr.kind else {
            panic!()
        };
        assert_eq!(items.len(), 4);
        assert_eq!(
            items[0],
            Expr {
                kind: Kind::Word("list".into()),
                offset: 1
            }
        );
        assert_eq!(items[1].kind, Kind::Word("甲".into()));
        assert_eq!(items[1].offset, source.find('甲').unwrap());
        let Kind::List(map) = &items[2].kind else {
            panic!()
        };
        assert_eq!(map[0].kind, Kind::Word("map".into()));
        assert_eq!(map[0].offset, source.find('{').unwrap());
        assert_eq!(map[1].offset, source.find("key").unwrap());
        assert_eq!(map[2].kind, Kind::Variable("value".into()));
        assert_eq!(map[2].offset, source.find('$').unwrap());
        assert_eq!(items[3].offset, source.find("(+").unwrap());
        for constructor in ["list", "map"] {
            let Read::Form(Expr {
                kind: Kind::List(items),
                ..
            }) = reader.read()
            else {
                panic!()
            };
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].kind, Kind::Word(constructor.into()));
        }
        assert!(
            matches!(reader.read(), Read::Form(Expr { kind: Kind::Word(word), .. }) if word == "tail")
        );
        assert_eq!(reader.read(), Read::End);
        assert!(complete("[{key [(+ 1 2)]}]"));
        assert!(!complete("[]{}"));
    }

    #[test]
    fn collection_delimiters_inside_strings_are_literal() {
        for delimiter in ["'", "\"", "'''", "\"\"\""] {
            let source = format!("[{delimiter}[{{)}}]{delimiter}]");
            let Read::Form(Expr {
                kind: Kind::List(items),
                ..
            }) = Reader::new(&source).read()
            else {
                panic!("{source}")
            };
            assert_eq!(items[1].kind, Kind::Text("[{)}]".into()));
        }
    }

    #[test]
    fn collection_delimiters_report_unclosed_mismatched_and_extra_closers() {
        for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
            let source = format!("{open}甲");
            assert_eq!(
                Reader::new(&source).read(),
                Read::Incomplete(Diagnostic {
                    offset: 0,
                    message: format!("unclosed '{open}'")
                })
            );
            for wrong in [')', ']', '}'].into_iter().filter(|&ch| ch != close) {
                let source = format!("{open}甲{wrong}");
                assert_eq!(
                    Reader::new(&source).read(),
                    Read::Error(Diagnostic {
                        offset: source.find(wrong).unwrap(),
                        message: format!("unexpected '{wrong}', expected '{close}'"),
                    })
                );
            }
        }
        assert!(matches!(Reader::new("(; [a)").read(), Read::Error(_)));
        let mut reader = Reader::new("]})[]");
        for (offset, close) in [']', '}', ')'].into_iter().enumerate() {
            assert_eq!(
                reader.read(),
                Read::ExtraClose(Diagnostic {
                    offset,
                    message: format!("unexpected '{close}'"),
                })
            );
        }
        assert!(matches!(reader.read(), Read::Form(_)));
        assert_eq!(reader.read(), Read::End);
    }

    #[test]
    fn collections_share_the_reader_depth_limit() {
        for (open, close) in [("[", "]"), ("{", "}")] {
            assert!(complete(&format!(
                "{}{}",
                open.repeat(256),
                close.repeat(256)
            )));
            let source = format!("{}{}", open.repeat(257), close.repeat(257));
            assert!(
                matches!(Reader::new(&source).read(), Read::Error(diagnostic)
                if diagnostic.message.contains("nesting exceeds 256"))
            );
        }
    }

    #[test]
    fn comments_are_structural_and_discarded() {
        let mut reader = Reader::new("(echo (; $missing (nonexistent x) (; nested)) a;b ; \"x)\")");
        let Read::Form(Expr {
            kind: Kind::List(items),
            ..
        }) = reader.read()
        else {
            panic!()
        };
        assert_eq!(items.len(), 4);
        assert_eq!(items[1].kind, Kind::Word("a;b".into()));
        assert_eq!(items[2].kind, Kind::Word(";".into()));
        assert_eq!(items[3].kind, Kind::Text("x)".into()));
    }

    #[test]
    fn extra_closers_are_individually_consumed() {
        let mut reader = Reader::new("(echo a))) (echo b)");
        assert!(matches!(reader.read(), Read::Form(_)));
        assert!(matches!(
            reader.read(),
            Read::ExtraClose(Diagnostic { offset: 8, .. })
        ));
        assert!(matches!(
            reader.read(),
            Read::ExtraClose(Diagnostic { offset: 9, .. })
        ));
        assert!(matches!(reader.read(), Read::Form(_)));
        assert_eq!(reader.read(), Read::End);
    }

    #[test]
    fn incomplete_strings_comments_and_nested_calls() {
        for source in ["(echo (pwd)", "(; (nested)", "(echo \"x)", "(echo \"x\\"] {
            assert!(
                matches!(Reader::new(source).read(), Read::Incomplete(_)),
                "{source}"
            );
        }
        assert!(complete("(echo \"x)\\\"\" (pwd))"));
        assert!(!complete("(echo a)(echo b)"));
    }

    #[test]
    fn interpolations_preserve_expression_offsets_and_nested_syntax() {
        let source = r#""hello $名字 $(str "a" (; ignored "$(missing)") (+ 1 2))!""#;
        let Read::Form(Expr {
            kind: Kind::Interpolated(parts),
            ..
        }) = Reader::new(source).read()
        else {
            panic!()
        };
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].kind, Kind::Text("hello ".into()));
        assert_eq!(parts[1].kind, Kind::Variable("名字".into()));
        assert_eq!(parts[1].offset, source.find("$名字").unwrap());
        assert_eq!(parts[3].offset, source.find("$(str").unwrap() + 1);
        let Kind::List(call) = &parts[3].kind else {
            panic!()
        };
        assert_eq!(call.len(), 3);
        assert_eq!(call[1].kind, Kind::Text("a".into()));
        assert_eq!(parts[4].kind, Kind::Text("!".into()));
    }

    #[test]
    fn single_quotes_are_raw_and_both_quotes_delimit_words() {
        let source = r#"'$name $(+ 1 2)\n\"()'"#;
        let Read::Form(expr) = Reader::new(source).read() else {
            panic!()
        };
        assert_eq!(expr.kind, Kind::Text(source[1..source.len() - 1].into()));
        let mut reader = Reader::new("a'b c'\"d\"");
        for kind in [
            Kind::Word("a".into()),
            Kind::Text("b c".into()),
            Kind::Text("d".into()),
        ] {
            let Read::Form(expr) = reader.read() else {
                panic!()
            };
            assert_eq!(expr.kind, kind);
        }
        assert_eq!(reader.read(), Read::End);
    }

    #[test]
    fn triple_quotes_preserve_whitespace_and_short_quote_runs() {
        let text = "\n  one \"two\" \"\" three 'four' ''\r\n\tend\n";
        for delimiter in ["\"\"\"", "'''"] {
            for text in ["", text] {
                let source = format!("{delimiter}{text}{delimiter}tail");
                let mut reader = Reader::new(&source);
                assert_eq!(
                    reader.read(),
                    Read::Form(Expr {
                        kind: Kind::Text(text.into()),
                        offset: 0,
                    }),
                    "{source}"
                );
                assert_eq!(reader.offset, delimiter.len() * 2 + text.len());
                assert!(
                    matches!(reader.read(), Read::Form(Expr { kind: Kind::Word(word), .. }) if word == "tail")
                );
                assert_eq!(reader.read(), Read::End);
            }
        }
    }

    #[test]
    fn triple_double_quotes_share_escapes_and_nested_interpolation_offsets() {
        let source = r#""""hello $名字
$(str """inner "quoted" $名字""" '''$missing\n''')!""""#;
        let Read::Form(Expr {
            kind: Kind::Interpolated(parts),
            ..
        }) = Reader::new(source).read()
        else {
            panic!()
        };
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].kind, Kind::Text("hello ".into()));
        assert_eq!(parts[0].offset, 3);
        assert_eq!(parts[1].kind, Kind::Variable("名字".into()));
        assert_eq!(parts[1].offset, source.find("$名字").unwrap());
        assert_eq!(parts[2].kind, Kind::Text("\n".into()));
        assert_eq!(parts[3].offset, source.find("$(str").unwrap() + 1);
        let Kind::List(call) = &parts[3].kind else {
            panic!()
        };
        assert!(matches!(&call[1].kind, Kind::Interpolated(_)));
        assert_eq!(call[2].kind, Kind::Text(r#"$missing\n"#.into()));
        assert_eq!(parts[4].kind, Kind::Text("!".into()));

        for (source, expected) in [
            (
                r#""""\""" \\ \n \r \t \$name \$(+ 1 2) ${name} $1 $!""""#,
                "\"\"\" \\ \n \r \t $name $(+ 1 2) ${name} $1 $!",
            ),
            (r#""""end\\""""#, "end\\"),
            (r#"'''$name $(+ 1 2)\n\"()\'''"#, r#"$name $(+ 1 2)\n\"()\"#),
        ] {
            assert_eq!(
                Reader::new(source).read(),
                Read::Form(Expr {
                    kind: Kind::Text(expected.into()),
                    offset: 0,
                }),
                "{source}"
            );
        }
    }

    #[test]
    fn unfinished_triple_quotes_wait_for_the_full_delimiter() {
        for delimiter in ["\"\"\"", "'''"] {
            for text in ["", "line\n  你好 )", "\\"] {
                for closing in 0..3 {
                    let source = format!("(str {delimiter}{text}{}", &delimiter[..closing]);
                    assert!(
                        matches!(
                            Reader::new(&source).read(),
                            Read::Incomplete(Diagnostic { offset: 5, .. })
                        ),
                        "{source}"
                    );
                }
            }
            assert!(complete(&format!("(; {delimiter}) $missing{delimiter})")));
        }
        let source = r#"(str """你好\q""")"#;
        assert!(matches!(Reader::new(source).read(), Read::Error(diagnostic)
            if diagnostic.offset == source.find('\\').unwrap()
                && diagnostic.message == "unknown string escape"));
        assert!(complete(r#"(str '''你好\q''')"#));
    }

    #[test]
    fn escaped_dollars_and_non_interpolation_prefixes_remain_text() {
        let source = r#""\$name \$(+ 1 2) ${name} $1 $!""#;
        let Read::Form(expr) = Reader::new(source).read() else {
            panic!()
        };
        assert_eq!(expr.kind, Kind::Text("$name $(+ 1 2) ${name} $1 $!".into()));
        for source in [r#"(str 'raw"#, r#"(str "$(+ 1"#, r#"(str "$(str "inner"#] {
            assert!(
                matches!(Reader::new(source).read(), Read::Incomplete(_)),
                "{source}"
            );
        }
        assert!(complete(r#"(str "$(str "inner" '(literal)')")"#));
    }

    #[test]
    fn nested_interpolations_share_the_reader_depth_limit() {
        for delimiter in ["\"", "\"\"\""] {
            let mut source = "(str done)".to_owned();
            for _ in 0..130 {
                source = format!("(str {delimiter}${source}{delimiter})");
            }
            assert!(
                matches!(Reader::new(&source).read(), Read::Error(diagnostic) if diagnostic.message.contains("nesting exceeds 256"))
            );
        }
    }

    #[test]
    fn diagnostics_count_unicode_columns() {
        assert_eq!(position("甲\n乙)", "甲\n乙".len()), (2, 2));
    }

    #[test]
    fn quasiquote_sugar_reads_as_plain_calls() {
        let source = "`(if (not ,condition) ,@body)";
        let Read::Form(expr) = Reader::new(source).read() else {
            panic!()
        };
        assert_eq!(expr.offset, 0);
        assert_eq!(
            expr.source(),
            "(quasiquote (if (not (unquote condition)) (unquote-splicing body)))"
        );
        let Kind::List(items) = &expr.kind else {
            panic!()
        };
        assert_eq!(items[0].kind, Kind::Word("quasiquote".into()));
        assert_eq!(items[1].offset, 1);
        assert!(matches!(
            Reader::new("`x").read(),
            Read::Form(Expr { kind: Kind::List(items), .. })
                if items[1].kind == Kind::Word("x".into())
        ));
    }

    #[test]
    fn commas_outside_quasiquote_remain_word_characters() {
        for source in ["(echo a,b)", "(echo ,@c)", "(echo ,)"] {
            let Read::Form(expr) = Reader::new(source).read() else {
                panic!()
            };
            assert_eq!(expr.source(), source);
        }
        let Read::Form(expr) = Reader::new("`(a,b)").read() else {
            panic!()
        };
        assert_eq!(expr.source(), "(quasiquote (a (unquote b)))");
    }

    #[test]
    fn unquote_requires_an_expression_after_the_comma() {
        let source = "`(a ,)";
        let Read::Error(diagnostic) = Reader::new(source).read() else {
            panic!()
        };
        assert_eq!(diagnostic.offset, source.find(',').unwrap());
        assert_eq!(
            diagnostic.message,
            "expected an expression after ',', found ')'"
        );
        assert!(matches!(Reader::new("`(a ,").read(), Read::Incomplete(_)));
        assert!(matches!(Reader::new("`(a ,@").read(), Read::Incomplete(_)));
    }

    #[test]
    fn explicit_quasiquote_also_enables_unquote_sugar() {
        let Read::Form(expr) = Reader::new("(quasiquote (list 1 ,(+ 1 2)))").read() else {
            panic!()
        };
        assert_eq!(expr.source(), "(quasiquote (list 1 (unquote (+ 1 2))))");
        let Read::Form(expr) = Reader::new("(list a,b)").read() else {
            panic!()
        };
        assert_eq!(expr.source(), "(list a,b)");
    }

    #[test]
    fn complete_accepts_a_top_level_template() {
        assert!(complete("`(a ,b ,@c)"));
        assert!(!complete("`(a"));
        assert!(!complete("`(a ,"));
    }

    #[test]
    fn source_renders_expressions_that_the_reader_reads_back() {
        for source in [
            "(str \"hello $name $(+ 1 2)\")",
            r#"(str "a\nb\t\"q\" \$x")"#,
            "(str '''raw text''')",
            "`(echo ,$name)",
        ] {
            let Read::Form(expr) = Reader::new(source).read() else {
                panic!("{source}")
            };
            let rendered = expr.source();
            let Read::Form(reparsed) = Reader::new(&rendered).read() else {
                panic!("{rendered}")
            };
            assert_eq!(reparsed.source(), rendered, "{source} -> {rendered}");
        }
    }
}
