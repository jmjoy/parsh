use std::sync::atomic::{AtomicUsize, Ordering};

use super::{Arguments, Env, Error, EvalResult, Mode, Scope, Shell, Signal, Value, host_path};
use crate::{
    host::MAX_OUTPUT_BYTES,
    reader::{Diagnostic, Expr, Kind, Read, Reader, variable_name},
    streams::Io,
};

pub(super) const NAMES: &[&str] = &[
    "form/kind",
    "form/value",
    "form/children",
    "form/list",
    "form/word",
    "form/variable",
    "form/from-value",
    "form/source",
    "gensym",
    "macroexpand-1",
    "read",
    "eval",
    "load",
];
static NEXT_SYMBOL: AtomicUsize = AtomicUsize::new(1);

const CORE_MACROS: &str = r#"
(defmacro when (condition &rest body) `(if ,$condition (do ,@$body)))
(defmacro unless (condition &rest body) `(if (not ,$condition) (do ,@$body)))
(defmacro cond (&rest clauses)
  (let result (quote #nil))
  (let index #0)
  (for clause (list/reverse $clauses)
    (let parts (form/children $clause))
    (if (< (list/len $parts) #2) (throw "cond clauses require a condition and a body"))
    (let condition (list/first $parts))
    (let otherwise (and (eq (form/kind $condition) word) (eq (form/value $condition) else)))
    (if $otherwise
      (do (if (!= $index #0) (throw "cond else clause must be last"))
          (set condition (quote #true))))
    (let body (list/rest $parts))
    (set result `(if ,$condition (do ,@$body) ,$result))
    (set index (+ $index #1)))
  $result)
(defmacro -> (value &rest steps)
  (let result $value)
  (for step $steps
    (if (eq (form/kind $step) list)
      (do (let parts (form/children $step))
          (if (empty? $parts) (throw "threading steps cannot be empty"))
          (set result (form/list (list/concat [(list/first $parts) $result] (list/rest $parts)))))
      (set result (form/list [$step $result]))))
  $result)
(defmacro ->> (value &rest steps)
  (let result $value)
  (for step $steps
    (if (eq (form/kind $step) list)
      (do (let parts (form/children $step))
          (if (empty? $parts) (throw "threading steps cannot be empty"))
          (set result (form/list (list/append $parts $result))))
      (set result (form/list [$step $result]))))
  $result)
"#;

impl Shell {
    pub(super) fn install_core_macros(&mut self) {
        let mut reader = Reader::new(CORE_MACROS);
        loop {
            match reader.read() {
                Read::End => break,
                Read::Form(expr) => {
                    self.execute(&expr).expect("valid core macro definitions");
                }
                _ => unreachable!("core macros are valid source"),
            }
        }
    }

    pub(super) fn forms_builtin(
        &mut self,
        expr: &Expr,
        name: &str,
        args: Arguments<'_>,
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        let values = args.values(self, env, io)?;
        let ready = args.with_values(&values);
        if name == "gensym" {
            let prefix = if values.is_empty() {
                "G".into()
            } else {
                ready.text(self, expr, 0, env, io)?
            };
            if !variable_name(&prefix) {
                return Err(Error::new(expr, "gensym prefix must be a valid variable name").into());
            }
            let id = NEXT_SYMBOL
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .map_err(|_| Error::new(expr, "generated identifier space exhausted"))?;
            return Ok(form(
                Kind::Generated {
                    name: format!("\0{prefix}_{id}"),
                    variable: false,
                },
                expr,
            ));
        }
        if name == "load" {
            let path = ready.text(self, expr, 0, env, io)?;
            return self.load_source(expr, &path, env, mode, io);
        }
        if name == "read" {
            let source = ready.text(self, expr, 0, env, io)?;
            let mut reader = Reader::new(&source);
            let mut found = None;
            loop {
                match reader.read() {
                    Read::End => break,
                    Read::Form(node) if node.kind == Kind::Comment => {}
                    Read::Form(node) if found.is_none() => found = Some(node),
                    Read::Form(_) => {
                        return Err(read_error(expr, "read expects exactly one form").into());
                    }
                    Read::Incomplete(diagnostic)
                    | Read::Error(diagnostic)
                    | Read::ExtraClose(diagnostic) => {
                        return Err(read_error(expr, diagnostic.render("<read>", &source)).into());
                    }
                }
            }
            return found
                .map(|node| Value::Form(Box::new(node)))
                .ok_or_else(|| read_error(expr, "read expects exactly one form").into());
        }
        if name == "form/from-value" {
            return Ok(Value::Form(Box::new(literal(&values[0], expr, 0)?)));
        }
        if name == "form/list" {
            let Value::List(items) = &values[0] else {
                return Err(Error::new(expr, "form/list expects a list of forms").into());
            };
            let nodes = items
                .iter()
                .map(|value| expect_form(value, expr).cloned())
                .collect::<Result<Vec<_>, _>>()?;
            let node = Expr {
                kind: Kind::List(nodes),
                offset: expr.offset,
            };
            check_depth(&node, expr)?;
            return Ok(Value::Form(Box::new(node)));
        }
        if matches!(name, "form/word" | "form/variable") {
            return Ok(Value::Form(Box::new(identifier(
                &values[0],
                name == "form/variable",
                expr,
            )?)));
        }
        let node = expect_form(&values[0], args.at(expr, 0))?;
        match name {
            "eval" => {
                let caller = Scope::parent(env).unwrap_or_else(|| env.clone());
                self.eval(node, &caller, mode, io)
                    .map_err(|signal| match signal {
                        Signal::Error(mut error) => {
                            if !error.exit {
                                error.message = format!("in eval: {}", error.message);
                                error.offset = expr.offset;
                            }
                            Signal::Error(error)
                        }
                        signal => signal,
                    })
            }
            "macroexpand-1" => {
                let Kind::List(parts) = &node.kind else {
                    return Ok(values[0].clone());
                };
                let Some(Expr {
                    kind: Kind::Word(name),
                    ..
                }) = parts.first()
                else {
                    return Ok(values[0].clone());
                };
                let Some(definition) = self.macros.get(name).cloned() else {
                    return Ok(values[0].clone());
                };
                let arguments = parts[1..]
                    .iter()
                    .map(|node| Value::Form(Box::new(node.clone())))
                    .collect();
                let expanded = self.function_body(
                    expr,
                    &format!("macro {name}"),
                    &definition,
                    arguments,
                    Mode::Normal,
                    io,
                )?;
                expect_form(&expanded, expr)?;
                Ok(expanded)
            }
            "form/source" => Ok(Value::Text(node.source())),
            "form/kind" => Ok(Value::Text(
                match &node.kind {
                    Kind::Word(_)
                    | Kind::Generated {
                        variable: false, ..
                    } => "word",
                    Kind::Variable(_) | Kind::Generated { variable: true, .. } => "variable",
                    Kind::Text(_) => "text",
                    Kind::Number(super::Number::Int(_)) => "int",
                    Kind::Number(super::Number::Float(_)) => "float",
                    Kind::Bool(_) => "bool",
                    Kind::Nil => "nil",
                    Kind::List(_) => "list",
                    Kind::Interpolated(_) => "interpolated",
                    Kind::Comment => "comment",
                }
                .into(),
            )),
            "form/children" => match &node.kind {
                Kind::List(items) | Kind::Interpolated(items) => Ok(Value::List(
                    items
                        .iter()
                        .map(|node| Value::Form(Box::new(node.clone())))
                        .collect(),
                )),
                _ => Err(
                    Error::new(expr, "form/children expects a list or interpolated form").into(),
                ),
            },
            "form/value" => Ok(match &node.kind {
                Kind::Word(text) | Kind::Text(text) | Kind::Variable(text) => {
                    Value::Text(text.clone())
                }
                Kind::Generated { name, .. } => Value::Text(name.trim_start_matches('\0').into()),
                Kind::Number(number) => Value::Number(*number),
                Kind::Bool(value) => Value::Bool(*value),
                Kind::Nil | Kind::Comment => Value::Nil,
                _ => return Err(Error::new(expr, "form/value expects an atomic form").into()),
            }),
            _ => unreachable!("unknown form operation"),
        }
    }

    fn load_source(
        &mut self,
        expr: &Expr,
        path: &str,
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        if self.syntax_only {
            return Err(Error {
                kind: "unsupported".into(),
                ..Error::new(expr, "load requires a filesystem host")
            }
            .into());
        }
        if path.is_empty() || path.contains('\0') {
            return Err(load_error(expr, "load expects a nonempty path without NUL").into());
        }
        if path.ends_with('/') {
            return Err(load_error(expr, "load expects a file path").into());
        }
        if self.load_paths.len() >= 32 {
            return Err(load_error(expr, "load nesting exceeds 32 files").into());
        }
        let base = if let Some(parent) = self.load_paths.last() {
            parent
                .rsplit_once('/')
                .map(|(directory, _)| if directory.is_empty() { "/" } else { directory })
                .unwrap_or("/")
                .to_owned()
        } else if self.host.is_some() {
            self.host_cwd.clone()
        } else {
            #[cfg(feature = "native")]
            {
                self.cwd.to_string_lossy().into_owned()
            }
            #[cfg(not(feature = "native"))]
            {
                return Err(load_error(expr, "load requires a filesystem host").into());
            }
        };
        let path = if self.host.is_some() {
            host_path(&base, path)
        } else {
            #[cfg(feature = "native")]
            {
                std::path::Path::new(&base)
                    .join(path)
                    .to_string_lossy()
                    .into_owned()
            }
            #[cfg(not(feature = "native"))]
            {
                return Err(load_error(expr, "load requires a filesystem host").into());
            }
        };
        let (path, bytes) = if let Some(host) = &self.host {
            let bytes = host
                .borrow_mut()
                .read_file(&base, &path)
                .map_err(|error| load_error(expr, error.message))?;
            (path, bytes)
        } else {
            #[cfg(feature = "native")]
            {
                use std::io::Read;
                let canonical = std::fs::canonicalize(&path)
                    .map_err(|error| load_error(expr, format!("{path}: {error}")))?;
                let file = std::fs::File::open(&canonical)
                    .map_err(|error| load_error(expr, format!("{path}: {error}")))?;
                let mut bytes = Vec::new();
                file.take(MAX_OUTPUT_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| load_error(expr, format!("{path}: {error}")))?;
                (canonical.to_string_lossy().into_owned(), bytes)
            }
            #[cfg(not(feature = "native"))]
            {
                return Err(load_error(expr, "load requires a filesystem host").into());
            }
        };
        if self.load_paths.contains(&path) {
            return Err(load_error(expr, format!("cyclic load: {path}")).into());
        }
        if bytes.len() > MAX_OUTPUT_BYTES {
            return Err(load_error(expr, "source file exceeds 8 MiB").into());
        }
        let source = String::from_utf8(bytes)
            .map_err(|_| load_error(expr, format!("{path}: source must be UTF-8")))?;
        let caller = Scope::parent(env).unwrap_or_else(|| env.clone());
        self.load_paths.push(path.clone());
        let result = (|| {
            let mut reader = Reader::new(&source);
            let mut forms = Vec::new();
            loop {
                match reader.read() {
                    Read::End => return self.sequence(&forms, &caller, mode, io),
                    Read::Form(node) if node.kind == Kind::Comment => {}
                    Read::Form(node) => {
                        if !node.is_top_level() {
                            return Err(Error::new(&node, "top-level expressions must be calls, collection expressions or literals").into());
                        }
                        forms.push(node);
                    }
                    Read::Incomplete(diagnostic)
                    | Read::Error(diagnostic)
                    | Read::ExtraClose(diagnostic) => {
                        return Err(read_error(
                            &Expr {
                                kind: Kind::Nil,
                                offset: diagnostic.offset,
                            },
                            diagnostic.message,
                        )
                        .into());
                    }
                }
            }
        })();
        self.load_paths.pop();
        result.map_err(|signal| match signal {
            Signal::Error(mut error) => {
                if !error.exit {
                    error.message = Diagnostic {
                        offset: error.offset,
                        message: error.message,
                    }
                    .render(&path, &source);
                    error.offset = expr.offset;
                }
                Signal::Error(error)
            }
            signal => signal,
        })
    }
}

fn form(kind: Kind, expr: &Expr) -> Value {
    Value::Form(Box::new(Expr {
        kind,
        offset: expr.offset,
    }))
}
fn read_error(expr: &Expr, message: impl Into<String>) -> Error {
    Error {
        kind: "read".into(),
        ..Error::new(expr, message)
    }
}
fn load_error(expr: &Expr, message: impl Into<String>) -> Error {
    Error {
        kind: "load".into(),
        ..Error::new(expr, message)
    }
}
fn expect_form<'a>(value: &'a Value, expr: &Expr) -> Result<&'a Expr, Error> {
    match value {
        Value::Form(form) => Ok(form),
        _ => Err(Error::new(expr, "expected a form")),
    }
}

fn identifier(value: &Value, variable: bool, expr: &Expr) -> Result<Expr, Error> {
    let text = match value {
        Value::Form(node) => match &node.kind {
            Kind::Generated { name, .. } => {
                return Ok(Expr {
                    kind: Kind::Generated {
                        name: name.clone(),
                        variable,
                    },
                    offset: expr.offset,
                });
            }
            Kind::Word(name) | Kind::Variable(name) => name.clone(),
            _ => return Err(Error::new(expr, "expected text or an identifier form")),
        },
        Value::Text(text) => text.clone(),
        _ => return Err(Error::new(expr, "expected text or an identifier form")),
    };
    let kind = if variable {
        if text != "?" && !variable_name(&text) {
            return Err(Error::new(expr, "invalid variable name"));
        }
        Kind::Variable(text)
    } else {
        let mut reader = Reader::new(&text);
        if !matches!(reader.read(), Read::Form(Expr { kind: Kind::Word(ref word), .. }) if word == &text)
            || !matches!(reader.read(), Read::End)
        {
            return Err(Error::new(expr, "form/word expects one bare word"));
        }
        Kind::Word(text)
    };
    Ok(Expr {
        kind,
        offset: expr.offset,
    })
}

fn literal(value: &Value, expr: &Expr, depth: usize) -> Result<Expr, Error> {
    if depth >= 128 {
        return Err(Error::new(expr, "literal form nesting exceeds 128 levels"));
    }
    let kind = match value {
        Value::Text(text) => Kind::Text(text.clone()),
        Value::Number(number) => Kind::Number(*number),
        Value::Bool(value) => Kind::Bool(*value),
        Value::Nil => Kind::Nil,
        Value::List(values) => {
            let mut items = vec![Expr {
                kind: Kind::Word("list".into()),
                offset: expr.offset,
            }];
            for value in values {
                items.push(literal(value, expr, depth + 1)?);
            }
            Kind::List(items)
        }
        Value::Map(values) => {
            let mut items = vec![Expr {
                kind: Kind::Word("map".into()),
                offset: expr.offset,
            }];
            for (key, value) in values {
                items.push(Expr {
                    kind: Kind::Text(key.clone()),
                    offset: expr.offset,
                });
                items.push(literal(value, expr, depth + 1)?);
            }
            Kind::List(items)
        }
        _ => {
            return Err(Error::new(
                expr,
                "form/from-value expects literal data: text, numbers, booleans, nil, lists or maps",
            ));
        }
    };
    Ok(Expr {
        kind,
        offset: expr.offset,
    })
}

fn check_depth(node: &Expr, expr: &Expr) -> Result<(), Error> {
    let mut pending = vec![(node, 0)];
    while let Some((node, depth)) = pending.pop() {
        if depth >= 256 {
            return Err(Error::new(expr, "form nesting exceeds 256 levels"));
        }
        if let Kind::List(children) | Kind::Interpolated(children) = &node.kind {
            pending.extend(children.iter().map(|child| (child, depth + 1)));
        }
    }
    Ok(())
}
