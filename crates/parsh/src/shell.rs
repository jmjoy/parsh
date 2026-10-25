use std::{
    cell::RefCell,
    cmp,
    collections::{HashMap, HashSet},
    ffi::OsString,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub use crate::number::Number;
use indexmap::IndexMap;

use crate::{
    completion::Candidate,
    host::{
        CommandOutput, ExecutionHost, FileMode, HostError, HostStreams, MAX_OUTPUT_BYTES,
        OutputChunk, Stream,
    },
    reader::{Expr, Kind, variable_name},
    streams::{HostFile, Io, Output, SharedHost},
};

#[cfg(feature = "native")]
use crate::{job::JobControl, process};
#[cfg(feature = "native")]
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

mod map;
mod type_command;

#[derive(Clone, Debug)]
pub enum Value {
    Text(String),
    Number(Number),
    Bool(bool),
    List(Vec<Value>),
    Map(IndexMap<String, Value>),
    /// Quoted source, produced by `quote` and `quasiquote`.
    Form(Box<Expr>),
    Struct {
        name: String,
        fields: IndexMap<String, Value>,
    },
    Nil,
    Status(i32),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Text(left), Self::Text(right)) => left == right,
            (Self::Number(left), Self::Number(right)) => left == right,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::List(left), Self::List(right)) => left == right,
            (Self::Map(left), Self::Map(right)) => left == right,
            // Forms compare as syntax; the reader offset of every node is irrelevant.
            (Self::Form(left), Self::Form(right)) => expr_eq(left, right),
            (
                Self::Struct {
                    name: left,
                    fields: left_fields,
                },
                Self::Struct {
                    name: right,
                    fields: right_fields,
                },
            ) => left == right && left_fields == right_fields,
            (Self::Nil, Self::Nil) => true,
            (Self::Status(left), Self::Status(right)) => left == right,
            _ => false,
        }
    }
}

/// Compare two expressions by syntax, ignoring the source offset of every node.
fn expr_eq(left: &Expr, right: &Expr) -> bool {
    match (&left.kind, &right.kind) {
        (Kind::Word(left), Kind::Word(right))
        | (Kind::Text(left), Kind::Text(right))
        | (Kind::Variable(left), Kind::Variable(right)) => left == right,
        (Kind::List(left), Kind::List(right))
        | (Kind::Interpolated(left), Kind::Interpolated(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| expr_eq(left, right))
        }
        (Kind::Comment, Kind::Comment) => true,
        _ => false,
    }
}

impl Value {
    pub fn text(&self) -> Result<String, String> {
        match self {
            Self::Text(text) => Ok(text.clone()),
            Self::Number(number) => Ok(number.to_string()),
            Self::Bool(boolean) => Ok(boolean.to_string()),
            Self::Nil => Ok(String::new()),
            Self::List(_) => Err("expected a scalar value, got a list".into()),
            Self::Map(_) => Err("expected a scalar value, got a map".into()),
            Self::Form(_) => Err("expected a scalar value, got a form".into()),
            Self::Struct { .. } => Err("expected a scalar value, got a struct".into()),
            Self::Status(_) => Err("a command status cannot be used as text".into()),
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Text(text) => format!("{text:?}").replace('$', "\\$"),
            Self::Number(number) => number.to_string(),
            Self::Bool(boolean) => boolean.to_string(),
            Self::List(values) => format!(
                "({})",
                values
                    .iter()
                    .map(Value::display)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Self::Map(values) => {
                let mut parts = vec!["map".into()];
                for (key, value) in values {
                    parts.push(Self::Text(key.clone()).display());
                    parts.push(value.display());
                }
                format!("({})", parts.join(" "))
            }
            Self::Form(form) => form.source(),
            Self::Struct { name, fields } => {
                let mut parts = vec![name.clone()];
                parts.extend(fields.values().map(Value::display));
                format!("({})", parts.join(" "))
            }
            Self::Nil => "nil".into(),
            Self::Status(code) => format!("status {code}"),
        }
    }
}

#[derive(Debug)]
pub struct Error {
    pub code: i32,
    pub offset: usize,
    pub message: String,
    pub exit: bool,
}

impl Error {
    fn new(expr: &Expr, message: impl Into<String>) -> Self {
        Self {
            code: 2,
            offset: expr.offset,
            message: message.into(),
            exit: false,
        }
    }
}

/// Loop jumps unwind expressions without becoming values or command failures.
#[derive(Debug)]
enum Signal {
    Error(Error),
    Break(Option<Value>),
    Continue,
}

impl From<Error> for Signal {
    fn from(error: Error) -> Self {
        Self::Error(error)
    }
}

impl Signal {
    fn into_error(self, expr: &Expr) -> Error {
        match self {
            Self::Error(error) => error,
            Self::Break(_) => Error::new(expr, "break used outside a loop body"),
            Self::Continue => Error::new(expr, "continue used outside a loop body"),
        }
    }
}

type EvalResult<T> = Result<T, Signal>;

type Env = Rc<RefCell<Scope>>;

#[derive(Default)]
struct Scope {
    variables: HashMap<String, Value>,
    parent: Option<Env>,
}

impl Scope {
    fn child(parent: &Env) -> Env {
        Rc::new(RefCell::new(Self {
            variables: HashMap::new(),
            parent: Some(parent.clone()),
        }))
    }

    fn get(env: &Env, name: &str) -> Option<Value> {
        let scope = env.borrow();
        scope.variables.get(name).cloned().or_else(|| {
            scope
                .parent
                .as_ref()
                .and_then(|parent| Self::get(parent, name))
        })
    }

    fn set(env: &Env, name: &str, value: &Value) -> bool {
        let mut scope = env.borrow_mut();
        if let Some(existing) = scope.variables.get_mut(name) {
            *existing = value.clone();
            true
        } else {
            scope
                .parent
                .as_ref()
                .is_some_and(|parent| Self::set(parent, name, value))
        }
    }
}

#[derive(Clone)]
struct Function {
    parameters: Vec<String>,
    body: Vec<Expr>,
    closure: Env,
}

/// Field names of a struct, in definition order.
#[derive(Clone)]
struct StructDef {
    fields: Vec<String>,
}

/// One macro invocation, bundling the pieces the expansion needs.
struct MacroCall<'a> {
    expr: &'a Expr,
    name: &'a str,
    definition: &'a Function,
    args: &'a [Expr],
}

/// How a command name resolves against a defined struct.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum StructKind {
    Constructor,
    Accessor,
    Predicate,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Condition,
}

pub struct Shell {
    #[cfg(feature = "native")]
    pub cwd: PathBuf,
    pub environment: HashMap<OsString, OsString>,
    root: Env,
    functions: HashMap<String, Function>,
    macros: HashMap<String, Function>,
    structs: HashMap<String, StructDef>,
    pub interrupted: Arc<AtomicBool>,
    #[cfg(feature = "native")]
    pub job: Option<JobControl>,
    host: Option<SharedHost>,
    host_cwd: String,
    output: Vec<OutputChunk>,
    output_bytes: usize,
    syntax_only: bool,
    depth: usize,
    loop_depth: usize,
    macro_depth: usize,
}

/// Expansion is recursive; keep the depth small enough for the debug-build stack.
const MACRO_EXPANSION_LIMIT: usize = 32;

const BUILTINS: &[&str] = &[
    "do",
    "let",
    "set",
    "get",
    "if",
    "for",
    "while",
    "break",
    "continue",
    "defn",
    "defmacro",
    "defstruct",
    "quote",
    "quasiquote",
    "unquote",
    "unquote-splicing",
    "list",
    "map",
    "mget",
    "mset",
    "mdel",
    "mhas",
    "mkeys",
    "mvals",
    "mlen",
    "str",
    "eq",
    "not",
    "glob",
    "cd",
    "export",
    "exit",
    "type",
    "+",
    "-",
    "*",
    "/",
    "=",
    "!=",
    "<",
    "<=",
    ">",
    ">=",
    "|",
    "|>",
    "|1>",
    "|>>",
    "|1>>",
    "<|",
    "|2>",
    "|2>>",
];

fn builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

fn requires_host(name: &str) -> bool {
    matches!(
        name,
        "glob" | "cd" | "|" | "|>" | "|1>" | "|>>" | "|1>>" | "<|" | "|2>" | "|2>>"
    )
}

/// Normalize virtual paths using the host's POSIX filesystem rules.
fn host_path(cwd: &str, path: &str) -> String {
    let full = if path.starts_with('/') {
        path.into()
    } else {
        format!("{cwd}/{path}")
    };
    let mut parts = Vec::new();
    for part in full.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    format!("/{}", parts.join("/"))
}

impl Shell {
    /// Create an isolated language session without filesystem or process access.
    pub fn syntax() -> Self {
        let root = Rc::new(RefCell::new(Scope::default()));
        root.borrow_mut()
            .variables
            .insert("args".into(), Value::List(Vec::new()));
        Self {
            #[cfg(feature = "native")]
            cwd: PathBuf::new(),
            environment: HashMap::new(),
            root,
            functions: HashMap::new(),
            macros: HashMap::new(),
            structs: HashMap::new(),
            interrupted: Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "native")]
            job: None,
            host: None,
            host_cwd: String::new(),
            output: Vec::new(),
            output_bytes: 0,
            syntax_only: true,
            depth: 0,
            loop_depth: 0,
            macro_depth: 0,
        }
    }

    /// Create a session with an isolated execution and filesystem backend.
    pub fn with_host(host: impl ExecutionHost + 'static) -> Result<Self, HostError> {
        Self::with_shared_host(Rc::new(RefCell::new(host)))
    }

    fn with_shared_host(host: SharedHost) -> Result<Self, HostError> {
        let cwd = host.borrow_mut().reset()?;
        let mut shell = Self::syntax();
        shell.environment.extend([
            ("HOME".into(), cwd.clone().into()),
            ("PWD".into(), cwd.clone().into()),
            ("LANG".into(), "C.UTF-8".into()),
            ("TERM".into(), "xterm-256color".into()),
        ]);
        shell.host = Some(host);
        shell.host_cwd = cwd;
        shell.syntax_only = false;
        Ok(shell)
    }

    pub(crate) fn reset_session(&mut self) -> Result<(), HostError> {
        *self = if let Some(host) = &self.host {
            Self::with_shared_host(host.clone())?
        } else {
            Self::syntax()
        };
        Ok(())
    }

    pub(crate) fn completion_commands(&mut self, prefix: &str) -> Vec<Candidate> {
        let mut candidates: Vec<_> = BUILTINS
            .iter()
            .copied()
            .filter(|name| !self.syntax_only || !requires_host(name))
            .chain(self.functions.keys().map(String::as_str))
            .chain(self.macros.keys().map(String::as_str))
            .filter(|name| name.starts_with(prefix))
            .map(Candidate::word)
            .collect();
        let struct_candidates: Vec<_> = self
            .structs
            .iter()
            .flat_map(|(name, definition)| {
                std::iter::once(name.clone())
                    .chain(
                        definition
                            .fields
                            .iter()
                            .map(move |field| format!("{name}.{field}")),
                    )
                    .chain(std::iter::once(format!("{name}?")))
            })
            .filter(|name| name.starts_with(prefix))
            .map(Candidate::word)
            .collect();
        candidates.extend(struct_candidates);
        if self.syntax_only {
            return candidates;
        }
        if let Some(host) = &self.host {
            candidates.extend(
                host.borrow_mut()
                    .commands()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|name| name.starts_with(prefix))
                    .map(Candidate::word),
            );
        } else {
            #[cfg(feature = "native")]
            if !prefix.contains('/') {
                let path = self.search_path();
                for directory in std::env::split_paths(&path) {
                    let directory = self.cwd.join(directory);
                    let Ok(entries) = fs::read_dir(directory) else {
                        continue;
                    };
                    for entry in entries.flatten() {
                        let Ok(name) = entry.file_name().into_string() else {
                            continue;
                        };
                        if name.starts_with(prefix) && executable_file(&entry.path()) {
                            candidates.push(Candidate::word(name));
                        }
                    }
                }
            }
        }
        if prefix.contains('/') {
            candidates.extend(self.completion_paths(prefix, false, true));
        }
        candidates
    }

    pub(crate) fn completion_variables(&self, prefix: &str) -> Vec<Candidate> {
        let root = self.root.borrow();
        root.variables
            .keys()
            .map(String::as_str)
            .chain(self.environment.keys().filter_map(|name| name.to_str()))
            .filter(|name| variable_name(name))
            .map(|name| format!("${name}"))
            .filter(|name| name.starts_with(prefix))
            .map(Candidate::word)
            .collect()
    }

    pub(crate) fn completion_paths(
        &mut self,
        prefix: &str,
        directories_only: bool,
        executable: bool,
    ) -> Vec<Candidate> {
        if self.syntax_only {
            return Vec::new();
        }
        let (directory, name) = prefix.rfind('/').map_or(("", prefix), |slash| {
            (&prefix[..=slash], &prefix[slash + 1..])
        });
        let matches = |entry: &str| {
            entry.starts_with(name) && (!entry.starts_with('.') || name.starts_with('.'))
        };
        let mut candidates = Vec::new();
        if let Some(host) = &self.host {
            let parent = host_path(&self.host_cwd, directory);
            let parent = if parent == "/" {
                parent
            } else {
                format!("{parent}/")
            };
            for path in host.borrow_mut().paths().unwrap_or_default() {
                let is_directory = path.ends_with('/');
                let Some(entry) = path.strip_prefix(&parent) else {
                    continue;
                };
                let entry = entry.trim_end_matches('/');
                if entry.is_empty()
                    || entry.contains('/')
                    || !matches(entry)
                    || ((directories_only || executable) && !is_directory)
                {
                    continue;
                }
                candidates.push(Candidate {
                    text: format!("{directory}{entry}{}", if is_directory { "/" } else { "" }),
                    directory: is_directory,
                });
            }
        } else {
            #[cfg(feature = "native")]
            {
                let parent = self
                    .cwd
                    .join(if directory.is_empty() { "." } else { directory });
                if let Ok(entries) = fs::read_dir(parent) {
                    for entry in entries.flatten() {
                        let Ok(name) = entry.file_name().into_string() else {
                            continue;
                        };
                        if !matches(&name) {
                            continue;
                        }
                        let path = entry.path();
                        let is_directory = path.is_dir();
                        if (directories_only && !is_directory)
                            || (executable && !is_directory && !executable_file(&path))
                        {
                            continue;
                        }
                        candidates.push(Candidate {
                            text: format!(
                                "{directory}{name}{}",
                                if is_directory { "/" } else { "" }
                            ),
                            directory: is_directory,
                        });
                    }
                }
            }
        }
        candidates
    }

    pub fn take_output(&mut self) -> Vec<OutputChunk> {
        std::mem::take(&mut self.output)
    }

    #[cfg(feature = "native")]
    pub fn new(args: Vec<String>) -> std::io::Result<Self> {
        let mut shell = Self::syntax();
        shell.root.borrow_mut().variables.insert(
            "args".into(),
            Value::List(args.into_iter().map(Value::Text).collect()),
        );
        shell.cwd = std::env::current_dir()?;
        shell.environment = std::env::vars_os().collect();
        shell
            .environment
            .insert("PWD".into(), shell.cwd.as_os_str().into());
        shell.syntax_only = false;
        Ok(shell)
    }

    #[cfg(feature = "native")]
    pub fn interactive(&mut self) -> std::io::Result<()> {
        self.job = Some(JobControl::new(self.interrupted.clone())?);
        Ok(())
    }

    pub fn execute(&mut self, expr: &Expr) -> Result<Value, Error> {
        self.interrupted.store(false, Ordering::Relaxed);
        self.output_bytes = 0;
        self.macro_depth = 0;
        self.eval(expr, &self.root.clone(), Mode::Normal, &Io::default())
            .map_err(|signal| signal.into_error(expr))
    }

    fn check(&self, expr: &Expr) -> Result<(), Error> {
        if self.interrupted.load(Ordering::Relaxed) {
            Err(Error {
                code: 130,
                offset: expr.offset,
                message: "interrupted".into(),
                exit: false,
            })
        } else {
            Ok(())
        }
    }

    fn value(&mut self, expr: &Expr, env: &Env, io: &Io) -> EvalResult<Value> {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let nested = Io {
            output: Output::Capture(buffer.clone()),
            host_output: None,
            ..io.clone()
        };
        let result = self.eval(expr, env, Mode::Normal, &nested)?;
        if matches!(result, Value::Status(_)) {
            let bytes = std::mem::take(&mut *buffer.lock().unwrap());
            let text = String::from_utf8(bytes).map_err(|_| {
                Error::new(
                    expr,
                    "captured output is not UTF-8; use a pipeline or redirection for binary data",
                )
            })?;
            Ok(Value::Text(text.trim_end_matches('\n').into()))
        } else {
            Ok(result)
        }
    }

    fn scalar(&mut self, expr: &Expr, env: &Env, io: &Io) -> EvalResult<String> {
        Ok(self
            .value(expr, env, io)?
            .text()
            .map_err(|message| Error::new(expr, message))?)
    }

    fn variable(&self, expr: &Expr, name: &str, env: &Env) -> Result<Value, Error> {
        Self::valid_name(expr, name)?;
        Scope::get(env, name)
            .or_else(|| {
                self.environment
                    .get(&OsString::from(name))
                    .map(|s| Value::Text(s.to_string_lossy().into_owned()))
            })
            .ok_or_else(|| Error::new(expr, format!("undefined variable ${name}")))
    }

    fn number(&mut self, expr: &Expr, env: &Env, io: &Io) -> EvalResult<Number> {
        match self.value(expr, env, io)? {
            Value::Number(number) => Ok(number),
            Value::Text(text) => {
                Ok(Number::parse(&text).map_err(|message| Error::new(expr, message))?)
            }
            _ => Err(Error::new(expr, "expected a number or numeric text").into()),
        }
    }

    fn arithmetic(
        &mut self,
        expr: &Expr,
        operator: &str,
        args: &[Expr],
        env: &Env,
        io: &Io,
    ) -> EvalResult<Value> {
        Self::arity(
            expr,
            args,
            usize::from(matches!(operator, "-" | "/")),
            usize::MAX,
        )?;
        let Some(first) = args.first() else {
            return Ok(Value::Number(Number::Int(i64::from(operator == "*"))));
        };
        let mut result = self.number(first, env, io)?;
        if args.len() == 1 {
            result = match operator {
                "-" => result.negate(),
                "/" => Number::Float(1.0).calculate("/", result),
                _ => Ok(result),
            }
            .map_err(|message| Error::new(first, format!("{operator}: {message}")))?;
        } else {
            for arg in &args[1..] {
                let other = self.number(arg, env, io)?;
                result = result
                    .calculate(operator, other)
                    .map_err(|message| Error::new(arg, format!("{operator}: {message}")))?;
            }
        }
        Ok(Value::Number(result))
    }

    fn condition(&mut self, expr: &Expr, env: &Env, io: &Io) -> EvalResult<bool> {
        match self.eval(expr, env, Mode::Condition, io)? {
            Value::Bool(value) => Ok(value),
            Value::Status(status) => Ok(status == 0),
            _ => Err(Error::new(
                expr,
                "a condition must be a boolean or command status; use eq to compare text",
            )
            .into()),
        }
    }

    fn sequence(
        &mut self,
        expressions: &[Expr],
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        let mut value = Value::Nil;
        for (index, expr) in expressions.iter().enumerate() {
            value = self.eval(
                expr,
                env,
                if index + 1 == expressions.len() {
                    mode
                } else {
                    Mode::Normal
                },
                io,
            )?;
        }
        Ok(value)
    }

    fn loop_body(
        &mut self,
        expressions: &[Expr],
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        self.loop_depth += 1;
        let result = self.sequence(expressions, env, mode, io);
        self.loop_depth -= 1;
        result
    }

    fn eval(&mut self, expr: &Expr, env: &Env, mode: Mode, io: &Io) -> EvalResult<Value> {
        self.check(expr)?;
        if self.depth >= 128 {
            return Err(Error::new(expr, "evaluation nesting exceeds 128 levels").into());
        }
        self.depth += 1;
        let result = self.eval_inner(expr, env, mode, io);
        self.depth -= 1;
        result
    }

    fn eval_inner(&mut self, expr: &Expr, env: &Env, mode: Mode, io: &Io) -> EvalResult<Value> {
        let items = match &expr.kind {
            Kind::Text(text) | Kind::Word(text) => return Ok(Value::Text(text.clone())),
            Kind::Variable(name) => return Ok(self.variable(expr, name, env)?),
            Kind::Interpolated(parts) => {
                let mut text = String::new();
                for part in parts {
                    text.push_str(&self.scalar(part, env, io)?);
                }
                return Ok(Value::Text(text));
            }
            Kind::Comment => return Ok(Value::Nil),
            Kind::List(items) => items,
        };
        let Some(head) = items.first() else {
            return Err(Error::new(expr, "empty command").into());
        };
        let name = match &head.kind {
            Kind::Text(name) | Kind::Word(name) => name.clone(),
            Kind::Variable(_) | Kind::Interpolated(_) => self.scalar(head, env, io)?,
            _ => {
                return Err(Error::new(
                    head,
                    "command name must be text; user functions are named commands",
                )
                .into());
            }
        };
        let args = &items[1..];
        if self.syntax_only && requires_host(&name) {
            return Err(Error::new(
                expr,
                format!(
                    "{name} requires native parsh; the syntax playground has no filesystem or external commands"
                ),
            ).into());
        }
        match name.as_str() {
            "do" => self.sequence(args, env, mode, io),
            "get" => {
                Self::arity(expr, args, 1, 1)?;
                let Value::Text(name) = self.value(&args[0], env, io)? else {
                    return Err(Error::new(&args[0], "get expects a text variable name").into());
                };
                Ok(self.variable(&args[0], &name, env)?)
            }
            "let" | "set" => {
                Self::arity(expr, args, 2, 2)?;
                let variable = Self::binding(&args[0])?;
                let value = self.value(&args[1], env, io)?;
                if name == "let" {
                    env.borrow_mut().variables.insert(variable.into(), value);
                } else if !Scope::set(env, variable, &value) {
                    if self.environment.contains_key(&OsString::from(variable)) {
                        self.environment.insert(
                            variable.into(),
                            value.text().map_err(|m| Error::new(expr, m))?.into(),
                        );
                    } else {
                        return Err(Error::new(
                            &args[0],
                            format!("cannot set undefined variable {variable}"),
                        )
                        .into());
                    }
                }
                Ok(Value::Nil)
            }
            "if" => {
                Self::arity(expr, args, 2, 3)?;
                if self.condition(&args[0], env, io)? {
                    self.eval(&args[1], env, mode, io)
                } else if args.len() == 3 {
                    self.eval(&args[2], env, mode, io)
                } else {
                    Ok(Value::Nil)
                }
            }
            "while" => {
                Self::arity(expr, args, 2, usize::MAX)?;
                let mut result = Value::Nil;
                while self.condition(&args[0], env, io)? {
                    self.check(expr)?;
                    let local = Scope::child(env);
                    match self.loop_body(&args[1..], &local, Mode::Normal, io) {
                        Ok(value) => result = value,
                        Err(Signal::Break(value)) => return Ok(value.unwrap_or(result)),
                        Err(Signal::Continue) => continue,
                        Err(error) => return Err(error),
                    }
                }
                Ok(result)
            }
            "break" | "continue" => {
                Self::arity(expr, args, 0, usize::from(name == "break"))?;
                if self.loop_depth == 0 {
                    return Err(Error::new(expr, format!("{name} used outside a loop body")).into());
                }
                if name == "break" {
                    let value = args
                        .first()
                        .map(|arg| self.value(arg, env, io))
                        .transpose()?;
                    Err(Signal::Break(value))
                } else {
                    Err(Signal::Continue)
                }
            }
            "for" => {
                Self::arity(expr, args, 3, usize::MAX)?;
                let variable = Self::binding(&args[0])?;
                let Value::List(values) = self.value(&args[1], env, io)? else {
                    return Err(Error::new(&args[1], "for expects a list").into());
                };
                let count = values.len();
                let mut result = Value::Nil;
                for (index, value) in values.into_iter().enumerate() {
                    self.check(expr)?;
                    let local = Scope::child(env);
                    local.borrow_mut().variables.insert(variable.into(), value);
                    match self.loop_body(
                        &args[2..],
                        &local,
                        if index + 1 == count {
                            mode
                        } else {
                            Mode::Normal
                        },
                        io,
                    ) {
                        Ok(value) => result = value,
                        Err(Signal::Break(value)) => return Ok(value.unwrap_or(result)),
                        Err(Signal::Continue) => continue,
                        Err(error) => return Err(error),
                    }
                }
                Ok(result)
            }
            "defn" => {
                Self::arity(expr, args, 3, usize::MAX)?;
                let Kind::Word(function_name) = &args[0].kind else {
                    return Err(Error::new(
                        &args[0],
                        "function name must be a bare command name without '$'",
                    )
                    .into());
                };
                if builtin(function_name) || function_name == ";" || function_name.is_empty() {
                    return Err(Error::new(&args[0], "function name is reserved").into());
                }
                if self.structs.contains_key(function_name) {
                    return Err(Error::new(
                        &args[0],
                        format!("{function_name} is already a struct"),
                    )
                    .into());
                }
                if self.macros.contains_key(function_name) {
                    return Err(Error::new(
                        &args[0],
                        format!("{function_name} is already a macro"),
                    )
                    .into());
                }
                if self.struct_member_kind(function_name).is_some() {
                    return Err(Error::new(
                        &args[0],
                        format!("{function_name} is a struct member"),
                    )
                    .into());
                }
                let names = Self::parameters(&args[1], "function")?;
                self.functions.insert(
                    function_name.clone(),
                    Function {
                        parameters: names,
                        body: args[2..].to_vec(),
                        closure: env.clone(),
                    },
                );
                Ok(Value::Nil)
            }
            "defmacro" => {
                self.defmacro(expr, args, env)?;
                Ok(Value::Nil)
            }
            "defstruct" => {
                self.defstruct(expr, args)?;
                Ok(Value::Nil)
            }
            "quote" => {
                Self::arity(expr, args, 1, 1)?;
                Ok(Value::Form(Box::new(args[0].clone())))
            }
            "quasiquote" => {
                Self::arity(expr, args, 1, 1)?;
                self.quasiquote(&args[0], env, io)
            }
            "unquote" => Err(Error::new(expr, "unquote is only valid inside a quasiquote").into()),
            "unquote-splicing" => {
                Err(Error::new(expr, "unquote-splicing is only valid inside a quasiquote").into())
            }
            "list" => args
                .iter()
                .map(|arg| self.value(arg, env, io))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List),
            "map" => self.construct_map(expr, args, env, io),
            "mget" | "mset" | "mdel" | "mhas" | "mkeys" | "mvals" | "mlen" => {
                self.map_command(expr, &name, args, env, io)
            }
            "str" => {
                let mut result = String::new();
                for arg in args {
                    result.push_str(&self.scalar(arg, env, io)?);
                }
                Ok(Value::Text(result))
            }
            "+" | "-" | "*" | "/" => self.arithmetic(expr, &name, args, env, io),
            "=" | "!=" | "<" | "<=" | ">" | ">=" => {
                Self::arity(expr, args, 2, 2)?;
                let left = self.number(&args[0], env, io)?;
                let right = self.number(&args[1], env, io)?;
                let result = match name.as_str() {
                    "=" => left.equals(right),
                    "!=" => !left.equals(right),
                    "<" => left.compare(right).is_some_and(cmp::Ordering::is_lt),
                    "<=" => left.compare(right).is_some_and(cmp::Ordering::is_le),
                    ">" => left.compare(right).is_some_and(cmp::Ordering::is_gt),
                    ">=" => left.compare(right).is_some_and(cmp::Ordering::is_ge),
                    _ => unreachable!("unknown comparison operator"),
                };
                Ok(Value::Bool(result))
            }
            "eq" => {
                Self::arity(expr, args, 2, 2)?;
                Ok(Value::Bool(
                    self.value(&args[0], env, io)? == self.value(&args[1], env, io)?,
                ))
            }
            "not" => {
                Self::arity(expr, args, 1, 1)?;
                Ok(Value::Bool(!self.condition(&args[0], env, io)?))
            }
            "type" => self.type_command(expr, args, env, mode, io),
            "glob" => {
                if self.host.is_some() {
                    return self.host_glob(expr, args, env, io);
                }
                #[cfg(feature = "native")]
                {
                    Self::arity(expr, args, 1, 1)?;
                    let pattern = self.scalar(&args[0], env, io)?;
                    let relative = Path::new(&pattern).is_relative();
                    let full = self.path(&pattern);
                    let options = glob::MatchOptions {
                        case_sensitive: true,
                        require_literal_separator: true,
                        require_literal_leading_dot: true,
                    };
                    let paths = glob::glob_with(&full.to_string_lossy(), options)
                        .map_err(|e| Error::new(expr, e.to_string()))?;
                    let mut values = Vec::new();
                    for path in paths {
                        self.check(expr)?;
                        let path = path.map_err(|e| Error::new(expr, e.to_string()))?;
                        let path = if relative {
                            path.strip_prefix(&self.cwd).unwrap_or(&path)
                        } else {
                            &path
                        };
                        values.push(path.to_string_lossy().into_owned());
                    }
                    values.sort();
                    Ok(Value::List(values.into_iter().map(Value::Text).collect()))
                }
                #[cfg(not(feature = "native"))]
                unreachable!("filesystem capability checked before dispatch")
            }
            "cd" => {
                if self.host.is_some() {
                    Self::arity(expr, args, 0, 1)?;
                    let destination = if args.is_empty() {
                        self.environment
                            .get(&OsString::from("HOME"))
                            .ok_or_else(|| Error::new(expr, "HOME is not set"))?
                            .to_string_lossy()
                            .into_owned()
                    } else {
                        self.scalar(&args[0], env, io)?
                    };
                    let cwd = self
                        .host
                        .as_ref()
                        .unwrap()
                        .borrow_mut()
                        .chdir(&self.host_cwd, &destination)
                        .map_err(|e| Self::host_error(expr, e))?;
                    self.environment
                        .insert("OLDPWD".into(), self.host_cwd.clone().into());
                    self.environment.insert("PWD".into(), cwd.clone().into());
                    self.host_cwd = cwd;
                    return Ok(Value::Nil);
                }
                #[cfg(feature = "native")]
                {
                    Self::arity(expr, args, 0, 1)?;
                    let destination = if args.is_empty() {
                        self.environment
                            .get(&OsString::from("HOME"))
                            .ok_or_else(|| Error::new(expr, "HOME is not set"))?
                            .to_string_lossy()
                            .into_owned()
                    } else {
                        self.scalar(&args[0], env, io)?
                    };
                    let path = fs::canonicalize(self.path(&destination))
                        .map_err(|e| Error::new(expr, format!("cd: {e}")))?;
                    if !path.is_dir() {
                        return Err(Error::new(expr, "cd: not a directory").into());
                    }
                    self.environment
                        .insert("OLDPWD".into(), self.cwd.as_os_str().into());
                    self.environment
                        .insert("PWD".into(), path.as_os_str().into());
                    self.cwd = path;
                    Ok(Value::Nil)
                }
                #[cfg(not(feature = "native"))]
                unreachable!("filesystem capability checked before dispatch")
            }
            "export" => {
                Self::arity(expr, args, 2, 2)?;
                let variable = Self::binding(&args[0])?;
                let value = Value::Text(self.scalar(&args[1], env, io)?);
                Scope::set(env, variable, &value);
                self.environment
                    .insert(variable.into(), value.text().unwrap().into());
                Ok(Value::Nil)
            }
            "exit" => {
                Self::arity(expr, args, 0, 1)?;
                let code = if args.is_empty() {
                    0
                } else {
                    self.scalar(&args[0], env, io)?
                        .parse::<u8>()
                        .map_err(|_| Error::new(expr, "exit expects a number from 0 to 255"))?
                        as i32
                };
                Err(Error {
                    code,
                    offset: expr.offset,
                    message: String::new(),
                    exit: true,
                }
                .into())
            }
            "|>" | "|1>" | "|>>" | "|1>>" | "<|" | "|2>" | "|2>>" => {
                if self.host.is_some() {
                    Self::arity(expr, args, 2, 2)?;
                    let destination = self.scalar(&args[0], env, io)?;
                    let file_mode = match name.as_str() {
                        "<|" => FileMode::Read,
                        "|>>" | "|1>>" | "|2>>" => FileMode::Append,
                        _ => FileMode::Write,
                    };
                    let host = self.host.as_ref().unwrap().clone();
                    let handle = host
                        .borrow_mut()
                        .open(&self.host_cwd, &destination, file_mode)
                        .map_err(|e| Self::host_error(&args[0], e))?;
                    let file = Some(Rc::new(HostFile { handle, host }));
                    let mut redirected = io.clone();
                    match name.as_str() {
                        "<|" => redirected.host_input = file,
                        "|2>" | "|2>>" => redirected.host_error = file,
                        _ => redirected.host_output = file,
                    }
                    return self.eval(&args[1], env, mode, &redirected);
                }
                #[cfg(feature = "native")]
                {
                    Self::arity(expr, args, 2, 2)?;
                    let destination = self.scalar(&args[0], env, io)?;
                    let path = self.path(&destination);
                    let append = matches!(name.as_str(), "|>>" | "|1>>" | "|2>>");
                    let file = if name == "<|" {
                        File::open(path)
                    } else {
                        OpenOptions::new()
                            .write(true)
                            .create(true)
                            .append(append)
                            .truncate(!append)
                            .open(path)
                    }
                    .map_err(|e| Error::new(&args[0], format!("{name}: {e}")))?;
                    let file = Arc::new(file);
                    let mut redirected = io.clone();
                    match name.as_str() {
                        "<|" => redirected.input = Some(file),
                        "|2>" | "|2>>" => redirected.error = Some(file),
                        _ => redirected.output = Output::File(file),
                    }
                    self.eval(&args[1], env, mode, &redirected)
                }
                #[cfg(not(feature = "native"))]
                unreachable!("filesystem capability checked before dispatch")
            }
            "|" => {
                Self::arity(expr, args, 1, usize::MAX)?;
                let mut stages = Vec::new();
                for stage in args {
                    let Kind::List(call) = &stage.kind else {
                        return Err(Error::new(
                            stage,
                            "pipeline stages must be external command calls",
                        )
                        .into());
                    };
                    let Some(head) = call.first() else {
                        return Err(Error::new(stage, "pipeline stage needs a command name").into());
                    };
                    let command = match &head.kind {
                        Kind::Text(command) | Kind::Word(command) => command.clone(),
                        Kind::Interpolated(_) => self.scalar(head, env, io)?,
                        _ => {
                            return Err(
                                Error::new(stage, "pipeline stage needs a command name").into()
                            );
                        }
                    };
                    if builtin(&command)
                        || self.functions.contains_key(&command)
                        || self.macros.contains_key(&command)
                        || self.struct_member_kind(&command).is_some()
                    {
                        return Err(Error::new(
                            stage,
                            "v1 pipeline stages must be external commands",
                        )
                        .into());
                    }
                    stages.push(self.argv(&command, &call[1..], env, io)?);
                }
                Ok(self.commands(expr, &stages, mode, io)?)
            }
            _ => {
                if let Some(definition) = self.macros.get(&name).cloned() {
                    let call = MacroCall {
                        expr,
                        name: &name,
                        definition: &definition,
                        args,
                    };
                    return self.expand_macro(&call, env, mode, io);
                }
                if let Some(fields) = self.structs.get(&name).map(|def| def.fields.clone()) {
                    Self::arity(expr, args, fields.len(), fields.len())?;
                    let mut values = IndexMap::new();
                    for (field, arg) in fields.into_iter().zip(args) {
                        let value = self.value(arg, env, io)?;
                        values.insert(field, value);
                    }
                    return Ok(Value::Struct {
                        name,
                        fields: values,
                    });
                }
                if let Some((struct_name, field)) = name.split_once('.')
                    && self.structs.contains_key(struct_name)
                {
                    Self::arity(expr, args, 1, 1)?;
                    let value = self.value(&args[0], env, io)?;
                    let Value::Struct {
                        name: value_name,
                        fields,
                    } = value
                    else {
                        return Err(Error::new(
                            expr,
                            format!("{name} expects a {struct_name} value"),
                        )
                        .into());
                    };
                    if value_name != struct_name {
                        return Err(Error::new(
                            expr,
                            format!("{name} expects a {struct_name} value"),
                        )
                        .into());
                    }
                    let field_value = fields.get(field).cloned().ok_or_else(|| {
                        Error::new(expr, format!("{struct_name} has no field {field}"))
                    })?;
                    return Ok(field_value);
                }
                if let Some(struct_name) = name.strip_suffix('?')
                    && self.structs.contains_key(struct_name)
                {
                    Self::arity(expr, args, 1, 1)?;
                    let value = self.value(&args[0], env, io)?;
                    return Ok(Value::Bool(matches!(
                        &value,
                        Value::Struct { name, .. } if name == struct_name
                    )));
                }
                if let Some(function) = self.functions.get(&name).cloned() {
                    Self::arity(
                        expr,
                        args,
                        function.parameters.len(),
                        function.parameters.len(),
                    )?;
                    let mut values = Vec::new();
                    for arg in args {
                        values.push(self.value(arg, env, io)?);
                    }
                    let local = Scope::child(&function.closure);
                    local
                        .borrow_mut()
                        .variables
                        .extend(function.parameters.into_iter().zip(values));
                    let caller_loop_depth = std::mem::take(&mut self.loop_depth);
                    let result = self.sequence(&function.body, &local, mode, io);
                    self.loop_depth = caller_loop_depth;
                    result.map_err(|signal| {
                        let mut error = signal.into_error(expr);
                        // Function bodies can originate in a different interactive input.
                        if !error.exit {
                            error.message = format!("in {name}: {}", error.message);
                            error.offset = expr.offset;
                        }
                        error.into()
                    })
                } else {
                    let argv = self.argv(&name, args, env, io)?;
                    Ok(self.commands(expr, &[argv], mode, io)?)
                }
            }
        }
    }

    /// Read a `(name ...)` parameter list of bare variable names.
    fn parameters(expr: &Expr, label: &str) -> Result<Vec<String>, Error> {
        let Kind::List(parameters) = &expr.kind else {
            return Err(Error::new(
                expr,
                format!("{label} parameters must be a list of bare variable names"),
            ));
        };
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        for parameter in parameters {
            let name = Self::binding(parameter)?;
            if !seen.insert(name) {
                return Err(Error::new(parameter, format!("duplicate parameter {name}")));
            }
            names.push(name.into());
        }
        Ok(names)
    }

    fn defmacro(&mut self, expr: &Expr, args: &[Expr], env: &Env) -> Result<(), Error> {
        Self::arity(expr, args, 3, usize::MAX)?;
        let Kind::Word(name) = &args[0].kind else {
            return Err(Error::new(
                &args[0],
                "macro name must be a bare command name without '$'",
            ));
        };
        if builtin(name) || name == ";" || name.is_empty() {
            return Err(Error::new(&args[0], "macro name is reserved"));
        }
        if self.structs.contains_key(name) {
            return Err(Error::new(&args[0], format!("{name} is already a struct")));
        }
        if self.functions.contains_key(name) {
            return Err(Error::new(
                &args[0],
                format!("{name} is already a function"),
            ));
        }
        if self.struct_member_kind(name).is_some() {
            return Err(Error::new(&args[0], format!("{name} is a struct member")));
        }
        let parameters = Self::parameters(&args[1], "macro")?;
        self.macros.insert(
            name.clone(),
            Function {
                parameters,
                body: args[2..].to_vec(),
                closure: env.clone(),
            },
        );
        Ok(())
    }

    fn defstruct(&mut self, expr: &Expr, args: &[Expr]) -> Result<(), Error> {
        if args.len() < 2 {
            return Err(Error::new(
                expr,
                "defstruct expects a name and at least one field",
            ));
        }
        let Kind::Word(name) = &args[0].kind else {
            return Err(Error::new(
                &args[0],
                "struct name must be a bare word without '$'",
            ));
        };
        if name.is_empty()
            || name == ";"
            || builtin(name)
            || name.chars().any(|ch| matches!(ch, '.' | '?' | '$'))
        {
            return Err(Error::new(&args[0], "struct name is reserved"));
        }
        if self.functions.contains_key(name) {
            return Err(Error::new(
                &args[0],
                format!("{name} is already a function"),
            ));
        }
        if self.macros.contains_key(name) {
            return Err(Error::new(&args[0], format!("{name} is already a macro")));
        }
        let mut fields = Vec::new();
        let mut seen = HashSet::new();
        for item in &args[1..] {
            let Kind::Word(field) = &item.kind else {
                return Err(Error::new(
                    item,
                    "field names must be bare names without '$'",
                ));
            };
            if !variable_name(field) {
                return Err(Error::new(
                    item,
                    "invalid field name; start with a letter or '_', then use letters, digits, or '_'",
                ));
            }
            if !seen.insert(field.clone()) {
                return Err(Error::new(item, format!("duplicate field {field}")));
            }
            fields.push(field.clone());
        }
        self.structs.insert(name.clone(), StructDef { fields });
        Ok(())
    }

    /// Resolve a command name into a struct constructor, accessor, or predicate.
    fn struct_member_kind(&self, name: &str) -> Option<StructKind> {
        if self.structs.contains_key(name) {
            return Some(StructKind::Constructor);
        }
        if let Some((struct_name, field)) = name.split_once('.')
            && self
                .structs
                .get(struct_name)
                .is_some_and(|definition| definition.fields.iter().any(|name| name == field))
        {
            return Some(StructKind::Accessor);
        }
        let struct_name = name.strip_suffix('?')?;
        self.structs
            .contains_key(struct_name)
            .then_some(StructKind::Predicate)
    }

    fn expand_macro(
        &mut self,
        call: &MacroCall<'_>,
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        let MacroCall {
            expr,
            name,
            definition,
            args,
        } = *call;
        Self::arity(
            expr,
            args,
            definition.parameters.len(),
            definition.parameters.len(),
        )?;
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(Value::Form(Box::new(arg.clone())));
        }
        let local = Scope::child(&definition.closure);
        local
            .borrow_mut()
            .variables
            .extend(definition.parameters.iter().cloned().zip(values));
        let caller_loop_depth = std::mem::take(&mut self.loop_depth);
        let expansion = self.sequence(&definition.body, &local, Mode::Normal, io);
        self.loop_depth = caller_loop_depth;
        let expansion = expansion.map_err(|signal| Self::macro_signal(signal, expr, name))?;
        let Value::Form(form) = expansion else {
            return Err(Error::new(expr, format!("macro {name} must return a quoted form")).into());
        };
        if self.macro_depth >= MACRO_EXPANSION_LIMIT {
            return Err(Error::new(
                expr,
                format!("macro expansion exceeds {MACRO_EXPANSION_LIMIT} levels"),
            )
            .into());
        }
        self.macro_depth += 1;
        let result = self.eval(&form, env, mode, io);
        self.macro_depth -= 1;
        result.map_err(|signal| Self::macro_signal(signal, expr, name))
    }

    fn macro_signal(signal: Signal, expr: &Expr, name: &str) -> Signal {
        let mut error = signal.into_error(expr);
        // Macro bodies can originate in a different interactive input, but nested
        // expansions of the same error must not pile up prefixes.
        if !error.exit && !error.message.starts_with("in macro ") {
            error.message = format!("in macro {name}: {}", error.message);
            error.offset = expr.offset;
        }
        error.into()
    }

    fn quasiquote(&mut self, template: &Expr, env: &Env, io: &Io) -> EvalResult<Value> {
        Ok(Value::Form(Box::new(self.template(template, env, io)?)))
    }

    /// Fill a quasiquote template with the evaluated unquote expressions.
    fn template(&mut self, template: &Expr, env: &Env, io: &Io) -> EvalResult<Expr> {
        let Kind::List(items) = &template.kind else {
            return Ok(template.clone());
        };
        if let Some(Kind::Word(head)) = items.first().map(|item| &item.kind) {
            match head.as_str() {
                "quasiquote" => return Ok(template.clone()),
                "unquote" => {
                    if items.len() != 2 {
                        return Err(
                            Error::new(template, "unquote expects exactly one expression").into(),
                        );
                    }
                    let value = self.value(&items[1], env, io)?;
                    return Ok(Self::form_node(value, &items[1])?);
                }
                "unquote-splicing" => {
                    return Err(Error::new(
                        template,
                        "unquote-splicing is only valid inside a list template",
                    )
                    .into());
                }
                _ => {}
            }
        }
        let mut expanded = Vec::with_capacity(items.len());
        for item in items {
            if let Kind::List(inner) = &item.kind
                && matches!(
                    inner.first().map(|expr| &expr.kind),
                    Some(Kind::Word(head)) if head == "unquote-splicing"
                )
            {
                if inner.len() != 2 {
                    return Err(Error::new(
                        item,
                        "unquote-splicing expects exactly one expression",
                    )
                    .into());
                }
                expanded.extend(self.splice(&inner[1], env, io)?);
            } else {
                expanded.push(self.template(item, env, io)?);
            }
        }
        Ok(Expr {
            kind: Kind::List(expanded),
            offset: template.offset,
        })
    }

    fn splice(&mut self, expr: &Expr, env: &Env, io: &Io) -> EvalResult<Vec<Expr>> {
        let message = "unquote-splicing expects a list form or a list of quoted forms";
        match self.value(expr, env, io)? {
            Value::Form(form) => match form.kind {
                Kind::List(items) => Ok(items),
                _ => Err(Error::new(expr, message).into()),
            },
            Value::List(values) => {
                let mut items = Vec::with_capacity(values.len());
                for value in values {
                    let Value::Form(form) = value else {
                        return Err(Error::new(expr, message).into());
                    };
                    items.push(*form);
                }
                Ok(items)
            }
            _ => Err(Error::new(expr, message).into()),
        }
    }

    /// Turn an unquoted value into the node that is spliced into the template.
    fn form_node(value: Value, expr: &Expr) -> Result<Expr, Error> {
        let kind = match value {
            Value::Form(form) => return Ok(*form),
            Value::Text(text) => Kind::Text(text),
            Value::Number(number) => Kind::Word(number.to_string()),
            Value::Bool(boolean) => Kind::Word(boolean.to_string()),
            Value::Nil => Kind::Word("nil".into()),
            _ => {
                return Err(Error::new(
                    expr,
                    "unquote expects a quoted form or a scalar value",
                ));
            }
        };
        Ok(Expr {
            kind,
            offset: expr.offset,
        })
    }

    fn argv(
        &mut self,
        command: &str,
        args: &[Expr],
        env: &Env,
        io: &Io,
    ) -> EvalResult<Vec<String>> {
        let mut argv = vec![command.into()];
        for arg in args {
            let value = self.value(arg, env, io)?;
            match value {
                Value::List(values) => {
                    for value in values {
                        argv.push(value.text().map_err(|m| Error::new(arg, m))?);
                    }
                }
                value => argv.push(value.text().map_err(|m| Error::new(arg, m))?),
            }
        }
        Ok(argv)
    }

    fn host_error(expr: &Expr, error: HostError) -> Error {
        Error {
            code: error.code,
            offset: expr.offset,
            message: error.message,
            exit: false,
        }
    }

    fn host_glob(&mut self, expr: &Expr, args: &[Expr], env: &Env, io: &Io) -> EvalResult<Value> {
        Self::arity(expr, args, 1, 1)?;
        let pattern = self.scalar(&args[0], env, io)?;
        let absolute = pattern.starts_with('/');
        let full = host_path(&self.host_cwd, &pattern);
        let matcher = glob::Pattern::new(&full).map_err(|e| Error::new(expr, e.to_string()))?;
        let options = glob::MatchOptions {
            case_sensitive: true,
            require_literal_separator: true,
            require_literal_leading_dot: true,
        };
        let paths = self
            .host
            .as_ref()
            .unwrap()
            .borrow_mut()
            .paths()
            .map_err(|e| Self::host_error(expr, e))?;
        let mut paths: Vec<_> = paths
            .into_iter()
            .filter(|path| {
                matcher.matches_with(
                    path.strip_suffix('/')
                        .filter(|path| !path.is_empty())
                        .unwrap_or(path),
                    options,
                )
            })
            .filter(|path| !pattern.ends_with('/') || path.ends_with('/'))
            .map(|path| {
                if path == "/" {
                    path
                } else {
                    path.trim_end_matches('/').to_string()
                }
            })
            .map(|path| {
                if absolute {
                    path
                } else {
                    relative_path(&self.host_cwd, &path)
                }
            })
            .collect();
        paths.sort();
        Ok(Value::List(paths.into_iter().map(Value::Text).collect()))
    }

    fn commands(
        &mut self,
        expr: &Expr,
        stages: &[Vec<String>],
        mode: Mode,
        io: &Io,
    ) -> Result<Value, Error> {
        self.check(expr)?;
        if self.syntax_only {
            return Err(Self::unavailable_command(expr, &stages[0][0]));
        }
        let code = if let Some(host) = &self.host {
            let streams = HostStreams {
                input: io.host_input.as_ref().map(|f| f.handle),
                output: io.host_output.as_ref().map(|f| f.handle),
                error: io.host_error.as_ref().map(|f| f.handle),
                capture: matches!(io.output, Output::Capture(_)),
            };
            let CommandOutput { code, chunks } = host
                .borrow_mut()
                .run(stages, &self.host_cwd, &self.environment, streams)
                .map_err(|e| Self::host_error(expr, e))?;
            for chunk in chunks {
                self.output_bytes = self.output_bytes.saturating_add(chunk.bytes.len());
                if self.output_bytes > MAX_OUTPUT_BYTES {
                    return Err(Error::new(expr, "command output exceeds 8 MiB"));
                }
                if chunk.stream == Stream::Stdout
                    && let Output::Capture(buffer) = &io.output
                {
                    buffer.lock().unwrap().extend(chunk.bytes);
                } else {
                    self.output.push(chunk);
                }
            }
            code
        } else {
            #[cfg(feature = "native")]
            {
                process::run(stages, &self.cwd, &self.environment, io, self.job.as_ref()).map_err(
                    |e| Error {
                        code: e.code,
                        offset: expr.offset,
                        message: e.message,
                        exit: false,
                    },
                )?
            }
            #[cfg(not(feature = "native"))]
            {
                return Err(Self::unavailable_command(expr, &stages[0][0]));
            }
        };
        Self::command_status(
            expr,
            &stages
                .iter()
                .map(|s| s[0].as_str())
                .collect::<Vec<_>>()
                .join(" | "),
            code,
            mode,
        )
    }

    fn command_status(expr: &Expr, name: &str, code: i32, mode: Mode) -> Result<Value, Error> {
        if code != 0 && mode != Mode::Condition {
            return Err(Error {
                code,
                offset: expr.offset,
                exit: false,
                message: format!("{name} exited with status {code}"),
            });
        }
        Ok(Value::Status(code))
    }

    /// Write builtin output through the same destinations as external commands.
    fn write_output(
        &mut self,
        expr: &Expr,
        io: &Io,
        stream: Stream,
        bytes: Vec<u8>,
    ) -> Result<(), Error> {
        self.output_bytes = self.output_bytes.saturating_add(bytes.len());
        if self.output_bytes > MAX_OUTPUT_BYTES {
            return Err(Error::new(expr, "command output exceeds 8 MiB"));
        }
        let host_file = match stream {
            Stream::Stdout => &io.host_output,
            Stream::Stderr => &io.host_error,
        };
        if let Some(file) = host_file {
            return file
                .host
                .borrow_mut()
                .write(file.handle, &bytes)
                .map_err(|error| Self::host_error(expr, error));
        }
        if stream == Stream::Stdout
            && let Output::Capture(buffer) = &io.output
        {
            buffer.lock().unwrap().extend(bytes);
            return Ok(());
        }
        if self.host.is_some() || self.syntax_only {
            self.output.push(OutputChunk { stream, bytes });
            return Ok(());
        }
        #[cfg(feature = "native")]
        {
            let result = match stream {
                Stream::Stdout => match &io.output {
                    Output::Inherit => std::io::stdout().lock().write_all(&bytes),
                    Output::File(file) => (&**file).write_all(&bytes),
                    Output::Capture(_) => unreachable!("captured above"),
                },
                Stream::Stderr => match &io.error {
                    Some(file) => (&**file).write_all(&bytes),
                    None => std::io::stderr().lock().write_all(&bytes),
                },
            };
            result.map_err(|error| Error::new(expr, format!("write: {error}")))
        }
        #[cfg(not(feature = "native"))]
        unreachable!("non-native output requires a host or syntax session")
    }

    fn unavailable_command(expr: &Expr, name: &str) -> Error {
        Error {
            code: 127,
            offset: expr.offset,
            exit: false,
            message: format!(
                "external command '{name}' requires native parsh; try (str \"hello\") to return text"
            ),
        }
    }

    fn binding(expr: &Expr) -> Result<&str, Error> {
        if let Kind::Word(name) = &expr.kind {
            Self::valid_name(expr, name)?;
            Ok(name)
        } else {
            Err(Error::new(
                expr,
                "binding must use a bare variable name without '$'",
            ))
        }
    }

    fn valid_name(expr: &Expr, name: &str) -> Result<(), Error> {
        if !variable_name(name) {
            Err(Error::new(
                expr,
                "invalid variable name; start with a letter or '_', then use letters, digits, or '_'",
            ))
        } else {
            Ok(())
        }
    }

    fn arity(expr: &Expr, args: &[Expr], min: usize, max: usize) -> Result<(), Error> {
        if args.len() >= min && args.len() <= max {
            Ok(())
        } else {
            let expected = if min == max {
                min.to_string()
            } else if max == usize::MAX {
                format!("at least {min}")
            } else {
                format!("{min}..{max}")
            };
            Err(Error::new(
                expr,
                format!("expected {expected} arguments, got {}", args.len()),
            ))
        }
    }

    #[cfg(feature = "native")]
    fn path(&self, path: &str) -> PathBuf {
        self.cwd.join(path)
    }

    #[cfg(feature = "native")]
    fn search_path(&self) -> OsString {
        // Match process execution's default when PATH is unset.
        self.environment
            .get(&OsString::from("PATH"))
            .cloned()
            .unwrap_or_else(|| "/bin:/usr/bin".into())
    }
}

#[cfg(feature = "native")]
fn executable_file(path: &Path) -> bool {
    path.is_file() && nix::unistd::access(path, nix::unistd::AccessFlags::X_OK).is_ok()
}

fn relative_path(cwd: &str, path: &str) -> String {
    let cwd: Vec<_> = cwd.split('/').filter(|part| !part.is_empty()).collect();
    let path: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
    let common = cwd.iter().zip(&path).take_while(|(a, b)| a == b).count();
    let parts: Vec<_> = std::iter::repeat_n("..", cwd.len() - common)
        .chain(path[common..].iter().copied())
        .collect();
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}
