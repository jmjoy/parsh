use std::cmp::Ordering;

use super::{Env, Error, EvalResult, Mode, Number, Shell, Value, bytes, http, json};
use crate::{reader::Expr, streams::Io};

pub(super) const VALUE_BUILTINS: &[&str] = &[
    "get",
    "dump",
    "list",
    "map",
    "map/get",
    "map/set",
    "map/del",
    "map/has",
    "map/keys",
    "map/vals",
    "map/len",
    "map/merge",
    "map/entries",
    "map/from-entries",
    "json/encode",
    "json/decode",
    "http/request",
    "http/get",
    "http/post",
    "http/body-text",
    "str",
    "int",
    "float",
    "bool",
    "eq",
    "not",
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
    "list/len",
    "list/first",
    "list/rest",
    "list/get",
    "list/append",
    "list/concat",
    "list/slice",
    "list/map",
    "list/filter",
    "list/fold",
    "list/any",
    "list/all",
    "list/reverse",
    "list/contains",
    "bytes/len",
    "bytes/slice",
    "bytes/concat",
    "bytes/from-text",
    "bytes/to-text",
    "bytes/base64-encode",
    "bytes/base64-decode",
    "bytes/hex-encode",
    "bytes/hex-decode",
    "str/len",
    "str/get",
    "str/slice",
    "str/split",
    "str/join",
    "str/lines",
    "str/trim",
    "str/replace",
    "str/contains",
    "typeof",
    "text?",
    "number?",
    "int?",
    "float?",
    "bool?",
    "list?",
    "map?",
    "form?",
    "struct?",
    "function?",
    "nil?",
    "status?",
    "empty?",
    "quot",
    "rem",
    "abs",
    "min",
    "max",
    "floor",
    "ceil",
    "round",
    "apply",
    "command",
    "command?",
    "callable?",
    "bytes?",
    "capture-call",
    "spawn",
    "wait",
    "job-status",
    "cancel",
    "check",
    "stdout-text",
    "stderr-text",
    "exit-code",
    "signal",
    "throw",
];

/// The same operations serve source calls and apply's already evaluated values.
#[derive(Clone, Copy)]
pub(super) enum Arguments<'a> {
    Expressions(&'a [Expr]),
    Values(&'a [Value]),
    Evaluated(&'a [Value], &'a [Expr]),
}

impl Arguments<'_> {
    pub(super) fn len(self) -> usize {
        match self {
            Self::Expressions(args) => args.len(),
            Self::Values(args) | Self::Evaluated(args, _) => args.len(),
        }
    }

    pub(super) fn at<'a>(&'a self, expr: &'a Expr, index: usize) -> &'a Expr {
        match self {
            Self::Expressions(args) | Self::Evaluated(_, args) => &args[index],
            Self::Values(_) => expr,
        }
    }

    pub(super) fn value(
        self,
        shell: &mut Shell,
        index: usize,
        env: &Env,
        io: &Io,
    ) -> EvalResult<Value> {
        match self {
            Self::Expressions(args) => shell.value(&args[index], env, io),
            Self::Values(values) | Self::Evaluated(values, _) => Ok(values[index].clone()),
        }
    }

    pub(super) fn values(self, shell: &mut Shell, env: &Env, io: &Io) -> EvalResult<Vec<Value>> {
        (0..self.len())
            .map(|i| self.value(shell, i, env, io))
            .collect()
    }

    pub(super) fn with_values<'a>(&'a self, values: &'a [Value]) -> Arguments<'a> {
        match self {
            Self::Expressions(exprs) | Self::Evaluated(_, exprs) => {
                Arguments::Evaluated(values, exprs)
            }
            Self::Values(_) => Arguments::Values(values),
        }
    }

    fn scalar(
        self,
        shell: &mut Shell,
        expr: &Expr,
        index: usize,
        env: &Env,
        io: &Io,
    ) -> EvalResult<String> {
        Ok(self
            .value(shell, index, env, io)?
            .text()
            .map_err(|m| Error::new(self.at(expr, index), m))?)
    }

    pub(super) fn text(
        self,
        shell: &mut Shell,
        expr: &Expr,
        index: usize,
        env: &Env,
        io: &Io,
    ) -> EvalResult<String> {
        match self.value(shell, index, env, io)? {
            Value::Text(text) => Ok(text),
            _ => Err(Error::new(self.at(expr, index), "expected text").into()),
        }
    }

    fn number(
        self,
        shell: &mut Shell,
        expr: &Expr,
        index: usize,
        env: &Env,
        io: &Io,
    ) -> EvalResult<Number> {
        match self.value(shell, index, env, io)? {
            Value::Number(number) => Ok(number),
            value => {
                let hint = if matches!(value, Value::Text(_)) {
                    "; use a # literal or int/float to convert text"
                } else {
                    ""
                };
                Err(Error::new(
                    self.at(expr, index),
                    format!(
                        "expected a number (int or float), got {}{hint}",
                        value.type_name()
                    ),
                )
                .into())
            }
        }
    }

    fn integer(
        self,
        shell: &mut Shell,
        expr: &Expr,
        index: usize,
        env: &Env,
        io: &Io,
    ) -> EvalResult<i64> {
        match self.value(shell, index, env, io)? {
            Value::Number(Number::Int(value)) => Ok(value),
            value => {
                let hint = if matches!(value, Value::Text(_)) {
                    "; use a # integer literal or int to convert text"
                } else {
                    ""
                };
                Err(Error::new(
                    self.at(expr, index),
                    format!("expected an integer, got {}{hint}", value.type_name()),
                )
                .into())
            }
        }
    }
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Number(Number::Int(_)) => "int",
            Self::Number(Number::Float(_)) => "float",
            Self::Bool(_) => "bool",
            Self::List(_) => "list",
            Self::Map(_) => "map",
            Self::Form(_) => "form",
            Self::Function(_) => "function",
            Self::Command(_) => "command",
            Self::Bytes(_) => "bytes",
            Self::Struct { .. } => "struct",
            Self::Nil => "nil",
            Self::Status(_) => "status",
        }
    }
}

fn arity(name: &str) -> (usize, usize) {
    match name {
        "list" | "str" | "map" | "+" | "*" | "list/concat" => (0, usize::MAX),
        "-" | "/" | "min" | "max" | "list/append" => (1, usize::MAX),
        "apply" => (2, usize::MAX),
        "capture-call" | "spawn" => (1, usize::MAX),
        "map/get" | "list/get" | "str/get" | "list/slice" | "str/slice" => (2, 3),
        "map/set" | "str/replace" => (3, 3),
        "map/merge" | "bytes/concat" => (0, usize::MAX),
        "map/entries"
        | "map/from-entries"
        | "list/reverse"
        | "bytes/len"
        | "bytes/from-text"
        | "bytes/to-text"
        | "bytes/base64-encode"
        | "bytes/base64-decode"
        | "bytes/hex-encode"
        | "bytes/hex-decode" => (1, 1),
        "list/contains" => (2, 2),
        "list/map" | "list/filter" | "list/any" | "list/all" => (2, 2),
        "list/fold" => (3, 3),
        "bytes/slice" => (2, 3),
        "http/request" => (2, 4),
        "http/get" => (1, 2),
        "http/post" => (2, 3),
        "json/encode" | "json/decode" | "http/body-text" => (1, 1),
        "eq" | "=" | "!=" | "<" | "<=" | ">" | ">=" | "map/del" | "map/has" | "quot" | "rem"
        | "str/split" | "str/join" | "str/contains" => (2, 2),
        "list/first" => (1, 2),
        _ => (1, 1),
    }
}

impl Shell {
    pub(super) fn call_builtin(
        &mut self,
        expr: &Expr,
        name: &str,
        args: Arguments<'_>,
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        let (min, max) = arity(name);
        Self::arity_count(expr, args.len(), min, max)?;
        match name {
            "json/encode" | "json/decode" => json::call(self, expr, name, args, env, mode, io),
            "bytes/len"
            | "bytes/slice"
            | "bytes/concat"
            | "bytes/from-text"
            | "bytes/to-text"
            | "bytes/base64-encode"
            | "bytes/base64-decode"
            | "bytes/hex-encode"
            | "bytes/hex-decode" => bytes::call(self, expr, name, args, env, mode, io),
            "http/request" | "http/get" | "http/post" | "http/body-text" => {
                http::call(self, expr, name, args, env, mode, io)
            }
            "command" | "command?" | "callable?" | "bytes?" | "capture-call" | "spawn" | "wait"
            | "job-status" | "cancel" | "check" | "stdout-text" | "stderr-text" | "exit-code"
            | "signal" | "throw" => self.execution_builtin(expr, name, args, env, mode, io),
            "get" => {
                let Value::Text(name) = args.value(self, 0, env, io)? else {
                    return Err(
                        Error::new(args.at(expr, 0), "get expects a text variable name").into(),
                    );
                };
                Ok(self.variable(args.at(expr, 0), &name, env)?)
            }
            "dump" => {
                let value = args.value(self, 0, env, io)?;
                self.write_output(
                    expr,
                    io,
                    crate::host::Stream::Stdout,
                    format!("{}\n", value.display()).into_bytes(),
                )?;
                Ok(Value::Nil)
            }
            "list" => Ok(Value::List(args.values(self, env, io)?)),
            "map" => self.construct_map(expr, args, env, io),
            "map/get" | "map/set" | "map/del" | "map/has" | "map/keys" | "map/vals" | "map/len" => {
                self.map_command(expr, name, args, env, io)
            }
            "map/merge" | "map/entries" | "map/from-entries" => {
                self.map_transform(expr, name, args, env, io)
            }
            "str" => {
                let mut result = String::new();
                for i in 0..args.len() {
                    result.push_str(&args.scalar(self, expr, i, env, io)?);
                }
                Ok(Value::Text(result))
            }
            "int" | "float" => {
                let error =
                    |message: String| Error::new(args.at(expr, 0), format!("{name}: {message}"));
                let number = match args.value(self, 0, env, io)? {
                    Value::Number(number) => number,
                    Value::Text(text) => if name == "float" {
                        Number::parse_float(&text)
                    } else {
                        Number::parse(&text)
                    }
                    .map_err(error)?,
                    _ => return Err(error("expected a number or numeric text".into()).into()),
                };
                let result = if name == "int" {
                    Number::Int(number.integer().map_err(|message| error(message.into()))?)
                } else {
                    Number::finite(number.as_float()).map_err(|message| error(message.into()))?
                };
                Ok(Value::Number(result))
            }
            "bool" => {
                let value = match args.value(self, 0, env, io)? {
                    Value::Bool(value) => Some(value),
                    Value::Text(text) => match text.trim() {
                        "true" => Some(true),
                        "false" => Some(false),
                        _ => None,
                    },
                    _ => None,
                }
                .ok_or_else(|| {
                    Error::new(
                        args.at(expr, 0),
                        "bool: expected a boolean or true/false text",
                    )
                })?;
                Ok(Value::Bool(value))
            }
            "+" | "-" | "*" | "/" => {
                if args.len() == 0 {
                    return Ok(Value::Number(Number::Int(i64::from(name == "*"))));
                }
                let mut result = args.number(self, expr, 0, env, io)?;
                if args.len() == 1 {
                    result = match name {
                        "-" => result.negate(),
                        "/" => Number::Float(1.0).calculate("/", result),
                        _ => Ok(result),
                    }
                    .map_err(|m| Error::new(args.at(expr, 0), format!("{name}: {m}")))?;
                } else {
                    for i in 1..args.len() {
                        let other = args.number(self, expr, i, env, io)?;
                        result = result
                            .calculate(name, other)
                            .map_err(|m| Error::new(args.at(expr, i), format!("{name}: {m}")))?;
                    }
                }
                Ok(Value::Number(result))
            }
            "=" | "!=" | "<" | "<=" | ">" | ">=" => {
                let left = args.number(self, expr, 0, env, io)?;
                let right = args.number(self, expr, 1, env, io)?;
                let order = left.compare(right);
                Ok(Value::Bool(match name {
                    "=" => left.equals(right),
                    "!=" => !left.equals(right),
                    "<" => order.is_some_and(Ordering::is_lt),
                    "<=" => order.is_some_and(Ordering::is_le),
                    ">" => order.is_some_and(Ordering::is_gt),
                    ">=" => order.is_some_and(Ordering::is_ge),
                    _ => unreachable!(),
                }))
            }
            "eq" => Ok(Value::Bool(
                args.value(self, 0, env, io)? == args.value(self, 1, env, io)?,
            )),
            "not" => {
                let condition = if let Arguments::Expressions(expressions) = args {
                    self.condition(&expressions[0], env, io)?
                } else {
                    match args.value(self, 0, env, io)? {
                        Value::Bool(value) => value,
                        Value::Status(status) => status.code == 0,
                        _ => return Err(Error::new(args.at(expr, 0), "a condition must be a boolean or command status; use eq to compare text").into()),
                    }
                };
                Ok(Value::Bool(!condition))
            }
            "apply" => {
                let mut values = args.values(self, env, io)?;
                let last = values.pop().unwrap();
                let callee = values.remove(0);
                let Value::List(tail) = last else {
                    return Err(Error::new(
                        args.at(expr, args.len() - 1),
                        "apply expects a final list argument",
                    )
                    .into());
                };
                values.extend(tail);
                self.call_value(expr, callee, Arguments::Values(&values), env, mode, io)
            }
            "list/map" | "list/filter" | "list/fold" | "list/any" | "list/all" | "list/reverse"
            | "list/contains" => self.list_transform(expr, name, args, env, mode, io),
            _ => {
                // New value operations evaluate explicit arguments in order before
                // inspecting their types or choosing a fallback result.
                let values = args.values(self, env, io)?;
                let ready = args.with_values(&values);
                if matches!(
                    name,
                    "quot" | "rem" | "abs" | "min" | "max" | "floor" | "ceil" | "round"
                ) {
                    return self.numeric_builtin(expr, name, ready, env, io);
                }
                if name.starts_with("list/") {
                    return self.list_builtin(expr, name, ready, env, io);
                }
                if name.starts_with("str/") {
                    return self.string_builtin(expr, name, ready, env, io);
                }
                let value = &values[0];
                if name == "typeof" {
                    return Ok(Value::Text(value.type_name().into()));
                }
                if name == "empty?" {
                    return Ok(Value::Bool(match value {
                        Value::Text(text) => text.is_empty(),
                        Value::List(items) => items.is_empty(),
                        Value::Map(items) => items.is_empty(),
                        _ => {
                            return Err(Error::new(
                                args.at(expr, 0),
                                "empty? expects text, a list, or a map",
                            )
                            .into());
                        }
                    }));
                }
                Ok(Value::Bool(if name == "number?" {
                    matches!(value, Value::Number(_))
                } else {
                    name.strip_suffix('?') == Some(value.type_name())
                }))
            }
        }
    }

    fn numeric_builtin(
        &mut self,
        expr: &Expr,
        name: &str,
        args: Arguments<'_>,
        env: &Env,
        io: &Io,
    ) -> EvalResult<Value> {
        if matches!(name, "quot" | "rem") {
            let a = args.integer(self, expr, 0, env, io)?;
            let b = args.integer(self, expr, 1, env, io)?;
            if b == 0 {
                return Err(
                    Error::new(args.at(expr, 1), format!("{name}: division by zero")).into(),
                );
            }
            let result = if name == "quot" {
                a.checked_div(b)
            } else {
                a.checked_rem(b)
            }
            .ok_or_else(|| Error::new(args.at(expr, 1), format!("{name}: integer overflow")))?;
            return Ok(Value::Number(Number::Int(result)));
        }
        let mut result = args.number(self, expr, 0, env, io)?;
        if matches!(name, "min" | "max") {
            for i in 1..args.len() {
                let other = args.number(self, expr, i, env, io)?;
                let order = other.compare(result);
                if (name == "min" && order == Some(Ordering::Less))
                    || (name == "max" && order == Some(Ordering::Greater))
                {
                    result = other;
                }
            }
        } else if name == "abs" {
            result = match result {
                Number::Int(value) => Number::Int(
                    value
                        .checked_abs()
                        .ok_or_else(|| Error::new(args.at(expr, 0), "abs: integer overflow"))?,
                ),
                Number::Float(value) => Number::Float(value.abs()),
            };
        } else if let Number::Float(value) = result {
            let rounded = match name {
                "floor" => value.floor(),
                "ceil" => value.ceil(),
                "round" => value.round(),
                _ => unreachable!(),
            };
            if !rounded.is_finite() || rounded < i64::MIN as f64 || rounded >= -(i64::MIN as f64) {
                return Err(Error::new(
                    args.at(expr, 0),
                    format!("{name}: integer out of range for i64"),
                )
                .into());
            }
            result = Number::Int(rounded as i64);
        }
        Ok(Value::Number(result))
    }

    fn index(
        &mut self,
        expr: &Expr,
        args: Arguments<'_>,
        index: usize,
        len: usize,
        env: &Env,
        io: &Io,
    ) -> EvalResult<i128> {
        let value = i128::from(args.integer(self, expr, index, env, io)?);
        Ok(if value < 0 {
            len as i128 + value
        } else {
            value
        })
    }

    fn slice_bounds(
        &mut self,
        expr: &Expr,
        args: Arguments<'_>,
        len: usize,
        env: &Env,
        io: &Io,
    ) -> EvalResult<(usize, usize)> {
        let start = self
            .index(expr, args, 1, len, env, io)?
            .clamp(0, len as i128) as usize;
        let end = if args.len() == 3 {
            self.index(expr, args, 2, len, env, io)?
                .clamp(0, len as i128) as usize
        } else {
            len
        };
        Ok((start, end.max(start)))
    }

    fn list_builtin(
        &mut self,
        expr: &Expr,
        name: &str,
        args: Arguments<'_>,
        env: &Env,
        io: &Io,
    ) -> EvalResult<Value> {
        if name == "list/concat" {
            let mut result = Vec::new();
            for i in 0..args.len() {
                let Value::List(items) = args.value(self, i, env, io)? else {
                    return Err(Error::new(args.at(expr, i), "expected a list").into());
                };
                result.extend(items);
            }
            return Ok(Value::List(result));
        }
        let Value::List(mut items) = args.value(self, 0, env, io)? else {
            return Err(Error::new(args.at(expr, 0), "expected a list").into());
        };
        match name {
            "list/len" => Ok(Value::Number(Number::Int(items.len() as i64))),
            "list/first" | "list/get" => {
                let index = if name == "list/first" {
                    0
                } else {
                    self.index(expr, args, 1, items.len(), env, io)?
                };
                if index >= 0 && index < items.len() as i128 {
                    return Ok(items.remove(index as usize));
                }
                let default = if name == "list/first" { 1 } else { 2 };
                if args.len() > default {
                    args.value(self, default, env, io)
                } else {
                    Err(Error::new(
                        args.at(expr, usize::from(name == "list/get")),
                        "list index out of range",
                    )
                    .into())
                }
            }
            "list/rest" => Ok(Value::List(items.into_iter().skip(1).collect())),
            "list/append" => {
                for i in 1..args.len() {
                    items.push(args.value(self, i, env, io)?);
                }
                Ok(Value::List(items))
            }
            "list/slice" => {
                let (start, end) = self.slice_bounds(expr, args, items.len(), env, io)?;
                Ok(Value::List(items.drain(start..end).collect()))
            }
            _ => unreachable!("unknown list builtin"),
        }
    }

    fn list_transform(
        &mut self,
        expr: &Expr,
        name: &str,
        args: Arguments<'_>,
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        match name {
            "list/reverse" => {
                let Value::List(mut values) = args.value(self, 0, env, io)? else {
                    return Err(Error::new(args.at(expr, 0), "list/reverse expects a list").into());
                };
                values.reverse();
                Ok(Value::List(values))
            }
            "list/contains" => {
                let Value::List(values) = args.value(self, 0, env, io)? else {
                    return Err(Error::new(args.at(expr, 0), "list/contains expects a list").into());
                };
                let needle = args.value(self, 1, env, io)?;
                Ok(Value::Bool(values.iter().any(|value| value == &needle)))
            }
            "list/map" | "list/filter" | "list/any" | "list/all" => {
                let callback = args.value(self, 0, env, io)?;
                let Value::List(values) = args.value(self, 1, env, io)? else {
                    return Err(
                        Error::new(args.at(expr, 1), "list transform expects a list").into(),
                    );
                };
                let mut mapped = Vec::with_capacity(values.len());
                for value in values {
                    let callback_args = [value.clone()];
                    let result = self.call_value(
                        expr,
                        callback.clone(),
                        Arguments::Values(&callback_args),
                        env,
                        mode,
                        io,
                    )?;
                    match name {
                        "list/map" => mapped.push(result),
                        "list/filter" | "list/any" | "list/all" => {
                            let Value::Bool(keep) = result else {
                                return Err(Error::new(
                                    args.at(expr, 0),
                                    format!("{name} predicate must return bool"),
                                )
                                .into());
                            };
                            if name == "list/filter" && keep {
                                mapped.push(value);
                            } else if name == "list/any" && keep {
                                return Ok(Value::Bool(true));
                            } else if name == "list/all" && !keep {
                                return Ok(Value::Bool(false));
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                Ok(match name {
                    "list/map" | "list/filter" => Value::List(mapped),
                    "list/any" => Value::Bool(false),
                    "list/all" => Value::Bool(true),
                    _ => unreachable!(),
                })
            }
            "list/fold" => {
                let callback = args.value(self, 0, env, io)?;
                let mut accumulator = args.value(self, 1, env, io)?;
                let Value::List(values) = args.value(self, 2, env, io)? else {
                    return Err(Error::new(args.at(expr, 2), "list/fold expects a list").into());
                };
                for value in values {
                    let callback_args = [accumulator, value];
                    accumulator = self.call_value(
                        expr,
                        callback.clone(),
                        Arguments::Values(&callback_args),
                        env,
                        mode,
                        io,
                    )?;
                }
                Ok(accumulator)
            }
            _ => unreachable!("unknown list transform"),
        }
    }

    fn string_builtin(
        &mut self,
        expr: &Expr,
        name: &str,
        args: Arguments<'_>,
        env: &Env,
        io: &Io,
    ) -> EvalResult<Value> {
        if name == "str/join" {
            let Value::List(items) = args.value(self, 0, env, io)? else {
                return Err(
                    Error::new(args.at(expr, 0), "str/join expects a list of text values").into(),
                );
            };
            let mut texts = Vec::new();
            for item in items {
                let Value::Text(text) = item else {
                    return Err(Error::new(
                        args.at(expr, 0),
                        "str/join expects a list of text values",
                    )
                    .into());
                };
                texts.push(text);
            }
            return Ok(Value::Text(texts.join(&args.text(self, expr, 1, env, io)?)));
        }
        let text = args.text(self, expr, 0, env, io)?;
        match name {
            "str/len" => Ok(Value::Number(Number::Int(text.chars().count() as i64))),
            "str/get" => {
                let len = text.chars().count();
                let index = self.index(expr, args, 1, len, env, io)?;
                if index >= 0 && index < len as i128 {
                    return Ok(Value::Text(
                        text.chars().nth(index as usize).unwrap().to_string(),
                    ));
                }
                if args.len() == 3 {
                    args.value(self, 2, env, io)
                } else {
                    Err(Error::new(args.at(expr, 1), "string index out of range").into())
                }
            }
            "str/slice" => {
                let (start, end) = self.slice_bounds(expr, args, text.chars().count(), env, io)?;
                Ok(Value::Text(
                    text.chars().skip(start).take(end - start).collect(),
                ))
            }
            "str/split" => {
                let separator = args.text(self, expr, 1, env, io)?;
                Ok(Value::List(
                    text.split(&separator)
                        .map(|part| Value::Text(part.into()))
                        .collect(),
                ))
            }
            "str/lines" => Ok(Value::List(
                text.lines().map(|line| Value::Text(line.into())).collect(),
            )),
            "str/trim" => Ok(Value::Text(text.trim().into())),
            "str/replace" => {
                let from = args.text(self, expr, 1, env, io)?;
                let to = args.text(self, expr, 2, env, io)?;
                Ok(Value::Text(text.replace(&from, &to)))
            }
            "str/contains" => Ok(Value::Bool(
                text.contains(&args.text(self, expr, 1, env, io)?),
            )),
            _ => unreachable!("unknown string builtin"),
        }
    }
}
