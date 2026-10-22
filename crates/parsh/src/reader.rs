use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: Kind,
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Word(String),
    Text(String),
    Variable(String),
    List(Vec<Expr>),
    Comment,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub offset: usize,
    pub message: String,
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

/// An incremental reader. An unexpected top-level ')' is always consumed.
pub struct Reader<'a> {
    source: &'a str,
    pub offset: usize,
}

impl<'a> Reader<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    pub fn read(&mut self) -> Read {
        self.space();
        if self.offset == self.source.len() {
            return Read::End;
        }
        if self.peek() == Some(')') {
            let offset = self.offset;
            self.bump();
            return Read::ExtraClose(Diagnostic {
                offset,
                message: "unexpected ')'".into(),
            });
        }
        match self.expr(0) {
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
            Some('(') => {
                let mut items = Vec::new();
                loop {
                    self.space();
                    match self.peek() {
                        None => return Err(self.error(offset, "unclosed '('", true)),
                        Some(')') => {
                            self.bump();
                            break;
                        }
                        _ => {
                            let item = self.expr(depth + 1)?;
                            if item.kind != Kind::Comment {
                                items.push(item);
                            }
                        }
                    }
                }
                if matches!(items.first().map(|e| &e.kind), Some(Kind::Word(s)) if s == ";") {
                    Kind::Comment
                } else {
                    Kind::List(items)
                }
            }
            Some('"') => {
                let mut text = String::new();
                loop {
                    match self.bump() {
                        None => return Err(self.error(offset, "unclosed string", true)),
                        Some('"') => break,
                        Some('\\') => {
                            let escape_at = self.offset - 1;
                            match self.bump() {
                                Some('n') => text.push('\n'),
                                Some('r') => text.push('\r'),
                                Some('t') => text.push('\t'),
                                Some('\\') => text.push('\\'),
                                Some('"') => text.push('"'),
                                None => {
                                    return Err(self.error(offset, "unclosed string escape", true));
                                }
                                Some(_) => {
                                    return Err(self.error(
                                        escape_at,
                                        "unknown string escape",
                                        false,
                                    ));
                                }
                            }
                        }
                        Some(ch) => text.push(ch),
                    }
                }
                Kind::Text(text)
            }
            Some(ch) => {
                let mut text = String::from(ch);
                while self.peek().is_some_and(word_character) {
                    text.push(self.bump().unwrap());
                }
                if let Some(name) = text.strip_prefix('$') {
                    Kind::Variable(name.into())
                } else {
                    Kind::Word(text)
                }
            }
            None => return Err(self.error(offset, "expected an expression", true)),
        };
        Ok(Expr { kind, offset })
    }
}

/// Bare words have the same boundaries in the reader and completion scanner.
pub(crate) fn word_character(ch: char) -> bool {
    !ch.is_whitespace() && !matches!(ch, '(' | ')' | '"')
}

/// True only when the source contains one complete list or comment and no tail.
pub fn complete(source: &str) -> bool {
    let mut reader = Reader::new(source);
    matches!(
        reader.read(),
        Read::Form(Expr {
            kind: Kind::List(_) | Kind::Comment,
            ..
        })
    ) && reader.read() == Read::End
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn diagnostics_count_unicode_columns() {
        assert_eq!(position("甲\n乙)", "甲\n乙".len()), (2, 2));
    }
}
