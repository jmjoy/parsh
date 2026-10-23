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
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Number(Number),
    Bool(bool),
    List(Vec<Value>),
    Nil,
    Status(i32),
}

impl Value {
    pub fn text(&self) -> Result<String, String> {
        match self {
            Self::Text(text) => Ok(text.clone()),
            Self::Number(number) => Ok(number.to_string()),
            Self::Bool(boolean) => Ok(boolean.to_string()),
            Self::Nil => Ok(String::new()),
            Self::List(_) => Err("expected a scalar value, got a list".into()),
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
}

const BUILTINS: &[&str] = &[
    "do", "let", "set", "get", "if", "for", "while", "break", "continue", "defn", "list", "str",
    "eq", "not", "glob", "cd", "export", "exit", "+", "-", "*", "/", "=", "!=", "<", "<=", ">",
    ">=", "|", "|>", "|1>", "|>>", "|1>>", "<|", "|2>", "|2>>",
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
            .filter(|name| name.starts_with(prefix))
            .map(Candidate::word)
            .collect();
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
                // Match the default command search path when PATH is unset.
                let path = self
                    .environment
                    .get(&OsString::from("PATH"))
                    .cloned()
                    .unwrap_or_else(|| "/bin:/usr/bin".into());
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
                let Kind::List(parameters) = &args[1].kind else {
                    return Err(Error::new(
                        &args[1],
                        "function parameters must be a list of bare variable names",
                    )
                    .into());
                };
                let mut names = Vec::new();
                let mut seen = HashSet::new();
                for parameter in parameters {
                    let name = Self::binding(parameter)?;
                    if !seen.insert(name) {
                        return Err(
                            Error::new(parameter, format!("duplicate parameter {name}")).into()
                        );
                    }
                    names.push(name.into());
                }
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
            "list" => args
                .iter()
                .map(|arg| self.value(arg, env, io))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List),
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
                    if builtin(&command) || self.functions.contains_key(&command) {
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
        if code != 0 && mode != Mode::Condition {
            return Err(Error {
                code,
                offset: expr.offset,
                exit: false,
                message: format!(
                    "{} exited with status {code}",
                    stages
                        .iter()
                        .map(|s| s[0].as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                ),
            });
        }
        Ok(Value::Status(code))
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
