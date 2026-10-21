use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{
    job::JobControl,
    process::{self, Io, Output},
    reader::{Expr, Kind},
};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Bool(bool),
    List(Vec<Value>),
    Nil,
    Status(i32),
}

impl Value {
    pub fn text(&self) -> Result<String, String> {
        match self {
            Self::Text(text) => Ok(text.clone()),
            Self::Bool(boolean) => Ok(boolean.to_string()),
            Self::Nil => Ok(String::new()),
            Self::List(_) => Err("expected a scalar value, got a list".into()),
            Self::Status(_) => Err("a command status cannot be used as text".into()),
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Text(text) => format!("{text:?}"),
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
    pub cwd: PathBuf,
    pub environment: HashMap<OsString, OsString>,
    root: Env,
    functions: HashMap<String, Function>,
    pub interrupted: Arc<AtomicBool>,
    pub job: Option<JobControl>,
    depth: usize,
}

fn builtin(name: &str) -> bool {
    matches!(
        name,
        "do" | "let"
            | "set"
            | "if"
            | "for"
            | "defn"
            | "list"
            | "str"
            | "eq"
            | "not"
            | "glob"
            | "cd"
            | "export"
            | "exit"
            | "|"
            | "|>"
            | "|>>"
            | "<|"
            | "|2>"
    )
}

impl Shell {
    pub fn new(args: Vec<String>) -> std::io::Result<Self> {
        let root = Rc::new(RefCell::new(Scope::default()));
        root.borrow_mut().variables.insert(
            "args".into(),
            Value::List(args.into_iter().map(Value::Text).collect()),
        );
        let cwd = std::env::current_dir()?;
        let mut environment: HashMap<OsString, OsString> = std::env::vars_os().collect();
        environment.insert("PWD".into(), cwd.as_os_str().into());
        Ok(Self {
            cwd,
            environment,
            root,
            functions: HashMap::new(),
            interrupted: Arc::new(AtomicBool::new(false)),
            job: None,
            depth: 0,
        })
    }

    pub fn interactive(&mut self) -> std::io::Result<()> {
        self.job = Some(JobControl::new(self.interrupted.clone())?);
        Ok(())
    }

    pub fn execute(&mut self, expr: &Expr) -> Result<Value, Error> {
        self.interrupted.store(false, Ordering::Relaxed);
        self.eval(expr, &self.root.clone(), Mode::Normal, &Io::default())
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

    fn value(&mut self, expr: &Expr, env: &Env, io: &Io) -> Result<Value, Error> {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let nested = Io {
            output: Output::Capture(buffer.clone()),
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

    fn scalar(&mut self, expr: &Expr, env: &Env, io: &Io) -> Result<String, Error> {
        self.value(expr, env, io)?
            .text()
            .map_err(|message| Error::new(expr, message))
    }

    fn condition(&mut self, expr: &Expr, env: &Env, io: &Io) -> Result<bool, Error> {
        match self.eval(expr, env, Mode::Condition, io)? {
            Value::Bool(value) => Ok(value),
            Value::Status(status) => Ok(status == 0),
            _ => Err(Error::new(
                expr,
                "a condition must be a boolean or command status; use eq to compare text",
            )),
        }
    }

    fn sequence(
        &mut self,
        expressions: &[Expr],
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> Result<Value, Error> {
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

    fn eval(&mut self, expr: &Expr, env: &Env, mode: Mode, io: &Io) -> Result<Value, Error> {
        self.check(expr)?;
        if self.depth >= 128 {
            return Err(Error::new(expr, "evaluation nesting exceeds 128 levels"));
        }
        self.depth += 1;
        let result = self.eval_inner(expr, env, mode, io);
        self.depth -= 1;
        result
    }

    fn eval_inner(&mut self, expr: &Expr, env: &Env, mode: Mode, io: &Io) -> Result<Value, Error> {
        let items = match &expr.kind {
            Kind::Text(text) | Kind::Word(text) => return Ok(Value::Text(text.clone())),
            Kind::Variable(name) => {
                Self::valid_name(expr, name)?;
                return Scope::get(env, name)
                    .or_else(|| {
                        self.environment
                            .get(&OsString::from(name))
                            .map(|s| Value::Text(s.to_string_lossy().into_owned()))
                    })
                    .ok_or_else(|| Error::new(expr, format!("undefined variable ${name}")));
            }
            Kind::Comment => return Ok(Value::Nil),
            Kind::List(items) => items,
        };
        let Some(head) = items.first() else {
            return Err(Error::new(expr, "empty command"));
        };
        let name = match &head.kind {
            Kind::Text(name) | Kind::Word(name) => name.clone(),
            Kind::Variable(_) => self.scalar(head, env, io)?,
            _ => {
                return Err(Error::new(
                    head,
                    "command name must be text; user functions are named commands",
                ));
            }
        };
        let args = &items[1..];
        match name.as_str() {
            "do" => self.sequence(args, env, mode, io),
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
                            format!("cannot set undefined variable ${variable}"),
                        ));
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
            "for" => {
                Self::arity(expr, args, 3, usize::MAX)?;
                let variable = Self::binding(&args[0])?;
                let Value::List(values) = self.value(&args[1], env, io)? else {
                    return Err(Error::new(&args[1], "for expects a list"));
                };
                let count = values.len();
                let mut result = Value::Nil;
                for (index, value) in values.into_iter().enumerate() {
                    self.check(expr)?;
                    let local = Scope::child(env);
                    local.borrow_mut().variables.insert(variable.into(), value);
                    result = self.sequence(
                        &args[2..],
                        &local,
                        if index + 1 == count {
                            mode
                        } else {
                            Mode::Normal
                        },
                        io,
                    )?;
                }
                Ok(result)
            }
            "defn" => {
                Self::arity(expr, args, 3, usize::MAX)?;
                let Kind::Word(function_name) = &args[0].kind else {
                    return Err(Error::new(
                        &args[0],
                        "function name must be a bare command name without '$'",
                    ));
                };
                if builtin(function_name) || function_name == ";" || function_name.is_empty() {
                    return Err(Error::new(&args[0], "function name is reserved"));
                }
                let Kind::List(parameters) = &args[1].kind else {
                    return Err(Error::new(
                        &args[1],
                        "function parameters must be a list of $variables",
                    ));
                };
                let mut names = Vec::new();
                let mut seen = HashSet::new();
                for parameter in parameters {
                    let name = Self::binding(parameter)?;
                    if !seen.insert(name) {
                        return Err(Error::new(
                            parameter,
                            format!("duplicate parameter ${name}"),
                        ));
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
            "cd" => {
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
                    return Err(Error::new(expr, "cd: not a directory"));
                }
                self.environment
                    .insert("OLDPWD".into(), self.cwd.as_os_str().into());
                self.environment
                    .insert("PWD".into(), path.as_os_str().into());
                self.cwd = path;
                Ok(Value::Nil)
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
                })
            }
            "|>" | "|>>" | "<|" | "|2>" => {
                Self::arity(expr, args, 2, 2)?;
                let destination = self.scalar(&args[0], env, io)?;
                let path = self.path(&destination);
                let file = if name == "<|" {
                    File::open(path)
                } else {
                    OpenOptions::new()
                        .write(true)
                        .create(true)
                        .append(name == "|>>")
                        .truncate(name != "|>>")
                        .open(path)
                }
                .map_err(|e| Error::new(&args[0], format!("{name}: {e}")))?;
                let file = Arc::new(file);
                let mut redirected = io.clone();
                match name.as_str() {
                    "<|" => redirected.input = Some(file),
                    "|2>" => redirected.error = Some(file),
                    _ => redirected.output = Output::File(file),
                }
                self.eval(&args[1], env, mode, &redirected)
            }
            "|" => {
                Self::arity(expr, args, 1, usize::MAX)?;
                let mut stages = Vec::new();
                for stage in args {
                    let Kind::List(call) = &stage.kind else {
                        return Err(Error::new(
                            stage,
                            "pipeline stages must be external command calls",
                        ));
                    };
                    let Some(Expr {
                        kind: Kind::Text(command) | Kind::Word(command),
                        ..
                    }) = call.first()
                    else {
                        return Err(Error::new(stage, "pipeline stage needs a command name"));
                    };
                    if builtin(command) || self.functions.contains_key(command) {
                        return Err(Error::new(
                            stage,
                            "v1 pipeline stages must be external commands",
                        ));
                    }
                    stages.push(self.argv(command, &call[1..], env, io)?);
                }
                self.commands(expr, &stages, mode, io)
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
                    self.sequence(&function.body, &local, mode, io)
                        .map_err(|mut error| {
                            // Function bodies can originate in a different interactive input.
                            if !error.exit {
                                error.message = format!("in {name}: {}", error.message);
                                error.offset = expr.offset;
                            }
                            error
                        })
                } else {
                    let argv = self.argv(&name, args, env, io)?;
                    self.commands(expr, &[argv], mode, io)
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
    ) -> Result<Vec<String>, Error> {
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

    fn commands(
        &mut self,
        expr: &Expr,
        stages: &[Vec<String>],
        mode: Mode,
        io: &Io,
    ) -> Result<Value, Error> {
        self.check(expr)?;
        let code = process::run(stages, &self.cwd, &self.environment, io, self.job.as_ref())
            .map_err(|e| Error {
                code: e.code,
                offset: expr.offset,
                message: e.message,
                exit: false,
            })?;
        if code != 0 && mode != Mode::Condition {
            return Err(Error {
                code,
                offset: expr.offset,
                message: format!(
                    "{} exited with status {code}",
                    stages
                        .iter()
                        .map(|s| s[0].as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                ),
                exit: false,
            });
        }
        Ok(Value::Status(code))
    }

    fn binding(expr: &Expr) -> Result<&str, Error> {
        if let Kind::Variable(name) = &expr.kind {
            Self::valid_name(expr, name)?;
            Ok(name)
        } else {
            Err(Error::new(expr, "binding must use a $variable"))
        }
    }

    fn valid_name(expr: &Expr, name: &str) -> Result<(), Error> {
        let mut chars = name.chars();
        if !chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
            || !chars.all(|c| c == '_' || c.is_alphanumeric())
        {
            Err(Error::new(
                expr,
                "invalid variable name; use a standalone $name and str for concatenation",
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

    fn path(&self, path: &str) -> PathBuf {
        self.cwd.join(path)
    }
}
