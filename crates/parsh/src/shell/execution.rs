//! Invocation results are data; checking them and projecting stdout are separate operations.
use super::*;

pub(super) const TYPES: &[(&str, &[&str])] = &[
    (
        "CallResult",
        &[
            "value",
            "termination",
            "stdout",
            "stderr",
            "error",
            "stages",
        ],
    ),
    ("Exited", &["code"]),
    ("Signaled", &["signal", "core_dumped"]),
    ("StageResult", &["argv", "termination", "error"]),
    ("Job", &["session", "id"]),
    ("JobStatus", &["state", "result"]),
    (
        "Error",
        &["kind", "message", "code", "offset", "payload", "result"],
    ),
    ("HttpResponse", &["status", "ok", "headers", "body", "url"]),
];

pub(super) fn record(name: &str, fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Struct {
        name: name.into(),
        fields: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    }
}

pub(super) fn field<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    match value {
        Value::Struct { fields, .. } => fields.get(name),
        _ => None,
    }
}

pub(super) fn is_record(value: &Value, expected: &str) -> bool {
    matches!(value, Value::Struct { name, .. } if name == expected)
}

pub(super) fn exited(code: i32) -> Value {
    record(
        "Exited",
        [("code", Value::Number(Number::Int(code.into())))],
    )
}

pub(super) fn termination(value: &crate::host::Termination) -> Value {
    match value {
        crate::host::Termination::Exited(code) => exited(*code),
        crate::host::Termination::Signaled {
            signal,
            core_dumped,
        } => record(
            "Signaled",
            [
                ("signal", Value::Number(Number::Int((*signal).into()))),
                ("core_dumped", Value::Bool(*core_dumped)),
            ],
        ),
    }
}

pub(super) fn result_record(value: Value, status: Value, error: Value, stages: Value) -> Value {
    record(
        "CallResult",
        [
            ("value", value),
            ("termination", status),
            ("stdout", Value::Nil),
            ("stderr", Value::Nil),
            ("error", error),
            ("stages", stages),
        ],
    )
}

fn set_field(value: &mut Value, key: &str, replacement: Value) {
    if let Value::Struct { fields, .. } = value {
        fields.insert(key.into(), replacement);
    }
}

impl Error {
    pub(super) fn as_value(&self, with_result: bool) -> Value {
        record(
            "Error",
            [
                ("kind", Value::Text(self.kind.clone())),
                ("message", Value::Text(self.message.clone())),
                ("code", Value::Number(Number::Int(self.code.into()))),
                ("offset", Value::Number(Number::Int(self.offset as i64))),
                ("payload", (*self.payload).clone()),
                (
                    "result",
                    if with_result {
                        self.result.as_deref().cloned().unwrap_or(Value::Nil)
                    } else {
                        Value::Nil
                    },
                ),
            ],
        )
    }

    fn from_value(expr: &Expr, value: &Value) -> Self {
        let text = |key| match field(value, key) {
            Some(Value::Text(s)) => s.clone(),
            _ => String::new(),
        };
        let number = |key, fallback| match field(value, key) {
            Some(Value::Number(Number::Int(n))) => *n,
            _ => fallback,
        };
        Self {
            kind: text("kind"),
            message: text("message"),
            code: number("code", 2) as i32,
            offset: number("offset", expr.offset as i64) as usize,
            payload: Box::new(field(value, "payload").cloned().unwrap_or(Value::Nil)),
            result: field(value, "result")
                .filter(|v| !matches!(v, Value::Nil))
                .cloned()
                .map(Box::new),
            exit: false,
            cancelled: false,
        }
    }
}

impl Shell {
    pub(super) fn install_execution_types(&mut self) {
        for &(name, fields) in TYPES {
            self.structs.insert(
                name.into(),
                StructDef {
                    fields: fields.iter().map(|s| (*s).into()).collect(),
                },
            );
        }
    }

    pub(super) fn publish(&mut self, value: Value) {
        self.last_result = value;
    }

    pub(super) fn capture_positions(io: &Io) -> (usize, usize) {
        let out = match &io.output {
            Output::Capture(b) => b.lock().unwrap().len(),
            _ => 0,
        };
        let err = io
            .error_capture
            .as_ref()
            .map_or(0, |b| b.lock().unwrap().len());
        (out, err)
    }

    fn captured(io: &Io, positions: (usize, usize)) -> (Value, Value) {
        let bytes = |b: &Arc<Mutex<Vec<u8>>>, start: usize| {
            let b = b.lock().unwrap();
            Value::Bytes(Arc::from(&b[start.min(b.len())..]))
        };
        let out = if io.host_output.is_none() {
            match &io.output {
                Output::Capture(b) => bytes(b, positions.0),
                _ => Value::Nil,
            }
        } else {
            Value::Nil
        };
        #[cfg(feature = "native")]
        let redirected_error = io.error.is_some() || io.host_error.is_some();
        #[cfg(not(feature = "native"))]
        let redirected_error = io.host_error.is_some();
        let err = if redirected_error {
            Value::Nil
        } else {
            io.error_capture
                .as_ref()
                .map_or(Value::Nil, |b| bytes(b, positions.1))
        };
        (out, err)
    }

    pub(super) fn publish_invocation(
        &mut self,
        expr: &Expr,
        outcome: &mut EvalResult<Value>,
        io: &Io,
        positions: (usize, usize),
    ) {
        let mut r = match outcome {
            Ok(Value::Status(status)) => (*status.result).clone(),
            Ok(v) => result_record(v.clone(), Value::Nil, Value::Nil, Value::List(Vec::new())),
            Err(Signal::Error(e)) if e.exit => {
                let error = if e.code == 0 {
                    Value::Nil
                } else {
                    Error {
                        code: e.code,
                        kind: "command-failed".into(),
                        ..Error::new(expr, format!("task exited with status {}", e.code))
                    }
                    .as_value(false)
                };
                result_record(Value::Nil, exited(e.code), error, Value::List(Vec::new()))
            }
            Err(Signal::Error(e)) => {
                let mut r = e.result.as_deref().cloned().unwrap_or_else(|| {
                    result_record(Value::Nil, Value::Nil, Value::Nil, Value::List(Vec::new()))
                });
                set_field(&mut r, "error", e.as_value(false));
                r
            }
            Err(_) => return,
        };
        let (out, err) = Self::captured(io, positions);
        set_field(&mut r, "stdout", out);
        set_field(&mut r, "stderr", err);
        match outcome {
            Err(Signal::Error(e)) => e.result = Some(Box::new(r.clone())),
            Ok(Value::Status(status)) => *status.result = r.clone(),
            _ => {}
        }
        self.publish(r);
    }

    pub(super) fn argv_values(
        expr: &Expr,
        command: &str,
        args: Vec<Value>,
    ) -> Result<Vec<String>, Error> {
        let mut result = vec![command.to_owned()];
        for value in args {
            match value {
                Value::List(items) => {
                    for item in items {
                        result.push(item.text().map_err(|m| Error::new(expr, m))?);
                    }
                }
                value => result.push(value.text().map_err(|m| Error::new(expr, m))?),
            }
        }
        Ok(result)
    }

    pub(super) fn resolve_callable(&self, expr: &Expr, value: Value) -> Result<Value, Error> {
        let Value::Text(name) = value else {
            return match value {
                Value::Function(_) | Value::Command(_) => Ok(value),
                _ => Err(Error::new(
                    expr,
                    "expected a callable value or text command name",
                )),
            };
        };
        if let Ok(function) = self.function_value(expr, &name) {
            return Ok(Value::Function(function));
        }
        if self.structs.contains_key(&name) || self.struct_member_kind(&name).is_some() {
            return Ok(Value::Text(name));
        }
        if builtin(&name) || self.macros.contains_key(&name) {
            return Err(Error::new(
                expr,
                format!("{name} is not an ordinary callable"),
            ));
        }
        Ok(Value::Command(name))
    }

    pub(super) fn call_value(
        &mut self,
        expr: &Expr,
        callee: Value,
        args: Arguments<'_>,
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        self.check(expr)?;
        if self.depth >= 128 {
            return Err(Error::new(expr, "evaluation nesting exceeds 128 levels").into());
        }
        self.depth += 1;
        let outcome = (|| match self.resolve_callable(expr, callee)? {
            Value::Function(f) => self.call_function(expr, &f, args, env, mode, io),
            Value::Command(name) => {
                let values = args.values(self, env, io)?;
                let argv = Self::argv_values(expr, &name, values)?;
                Ok(self.commands(expr, &[argv], mode, io)?)
            }
            Value::Text(name) => {
                // Bind already evaluated values, never turn them back into source.
                let values = args.values(self, env, io)?;
                let local = self.child_scope(env);
                let expressions: Vec<_> = values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let key = format!("argument_{index}");
                        local.borrow_mut().variables.insert(key.clone(), value);
                        Expr {
                            kind: Kind::Variable(key),
                            offset: expr.offset,
                        }
                    })
                    .collect();
                self.eval_named(expr, &name, &expressions, &local, mode, io)
            }
            _ => unreachable!(),
        })();
        self.depth -= 1;
        outcome
    }

    pub(super) fn capture_call(
        &mut self,
        expr: &Expr,
        callee: Value,
        values: &[Value],
        env: &Env,
        io: &Io,
        all_errors: bool,
    ) -> EvalResult<Value> {
        // Explicit nested captures allocate buffers and keep a full invocation frame alive.
        // Bound them independently of ordinary evaluation, as with macro expansion.
        if self.capture_depth >= 32 {
            return Err(Error::new(expr, "capture nesting exceeds 32 levels").into());
        }
        self.capture_depth += 1;
        let result = {
            let stdout = Arc::new(Mutex::new(Vec::new()));
            let stderr = Arc::new(Mutex::new(Vec::new()));
            let mut nested = io.clone();
            #[cfg(feature = "native")]
            let file_output = matches!(nested.output, Output::File(_));
            #[cfg(not(feature = "native"))]
            let file_output = false;
            if !file_output {
                nested.output = Output::Capture(stdout);
            }
            nested.error_capture = Some(stderr);
            nested.capture_budget = Some(Arc::new(crate::streams::CaptureBudget::default()));
            let positions = Self::capture_positions(&nested);
            let mut outcome = self.call_value(
                expr,
                callee,
                Arguments::Values(values),
                env,
                Mode::Condition,
                &nested,
            );
            self.publish_invocation(expr, &mut outcome, &nested, positions);
            match outcome {
                Ok(_) => Ok(self.last_result.clone()),
                Err(Signal::Error(e))
                    if all_errors
                        || (!e.exit
                            && matches!(e.kind.as_str(), "command-failed" | "signaled")) =>
                {
                    Ok(self.last_result.clone())
                }
                Err(signal) => Err(signal),
            }
        };
        self.capture_depth -= 1;
        result
    }

    pub(super) fn execution_builtin(
        &mut self,
        expr: &Expr,
        name: &str,
        args: Arguments<'_>,
        env: &Env,
        _mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        let mut values = args.values(self, env, io)?;
        match name {
            "command" => match values.remove(0) {
                Value::Text(s) if !s.is_empty() && !s.contains('\0') => Ok(Value::Command(s)),
                _ => Err(Error::new(
                    expr,
                    "command expects a nonempty text program name without NUL",
                )
                .into()),
            },
            "command?" => Ok(Value::Bool(matches!(values[0], Value::Command(_)))),
            "bytes?" => Ok(Value::Bool(matches!(values[0], Value::Bytes(_)))),
            "callable?" => Ok(Value::Bool(matches!(
                values[0],
                Value::Function(_) | Value::Command(_)
            ))),
            "capture-call" => {
                let callee = values.remove(0);
                self.capture_call(expr, callee, &values, env, io, false)
            }
            "spawn" | "wait" | "job-status" | "cancel" => {
                #[cfg(feature = "native")]
                if !self.syntax_only && self.host.is_none() {
                    return self.task_builtin(expr, name, values, env, io);
                }
                Err(Error {
                    kind: "unsupported".into(),
                    ..Error::new(
                        expr,
                        format!("{name} requires native background-task support"),
                    )
                }
                .into())
            }
            "throw" => {
                let payload = values.remove(0);
                let mut error = if is_record(&payload, "Error") {
                    Error::from_value(expr, &payload)
                } else {
                    Error {
                        kind: "user".into(),
                        message: format!("thrown: {}", payload.display()),
                        payload: Box::new(payload),
                        ..Error::new(expr, "")
                    }
                };
                let r = error.result.as_deref().cloned().unwrap_or_else(|| {
                    result_record(
                        Value::Nil,
                        Value::Nil,
                        error.as_value(false),
                        Value::List(Vec::new()),
                    )
                });
                error.result = Some(Box::new(r.clone()));
                self.publish(r);
                Err(error.into())
            }
            "check" => {
                let r = values.remove(0);
                Self::expect_result(expr, &r)?;
                self.publish(r.clone());
                if let Some(e) = field(&r, "error").filter(|e| !matches!(e, Value::Nil)) {
                    let mut error = Error::from_value(expr, e);
                    error.result = Some(Box::new(r));
                    return Err(error.into());
                }
                Ok(r)
            }
            "exit-code" | "signal" => {
                Self::expect_result(expr, &values[0])?;
                let status = field(&values[0], "termination").unwrap();
                let (expected, key) = if name == "exit-code" {
                    ("Exited", "code")
                } else {
                    ("Signaled", "signal")
                };
                Ok(if is_record(status, expected) {
                    field(status, key).cloned().unwrap_or(Value::Nil)
                } else {
                    Value::Nil
                })
            }
            "stdout-text" | "stderr-text" => {
                let r = &values[0];
                Self::expect_result(expr, r)?;
                let key = if name == "stdout-text" {
                    "stdout"
                } else {
                    "stderr"
                };
                match field(r, key) {
                    Some(Value::Nil) => Ok(Value::Nil),
                    Some(Value::Bytes(b)) => Ok(Value::Text(
                        String::from_utf8(b.to_vec()).map_err(|_| Error {
                            result: Some(Box::new(r.clone())),
                            ..Error::new(expr, format!("{key} is not UTF-8"))
                        })?,
                    )),
                    _ => Err(Error::new(expr, "invalid captured output").into()),
                }
            }
            _ => unreachable!(),
        }
    }

    fn expect_result(expr: &Expr, value: &Value) -> Result<(), Error> {
        if is_record(value, "CallResult") {
            Ok(())
        } else {
            Err(Error::new(expr, "expected a CallResult"))
        }
    }

    pub(super) fn try_expression(
        &mut self,
        expr: &Expr,
        args: &[Expr],
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        let mut body_end = args.len();
        let mut catch = None;
        let mut finally = None;
        for (index, arg) in args.iter().enumerate() {
            let clause = match &arg.kind {
                Kind::List(items) => match items.first().map(|h| &h.kind) {
                    Some(Kind::Word(name)) if matches!(name.as_str(), "catch" | "finally") => {
                        Some((name.as_str(), items))
                    }
                    _ => None,
                },
                _ => None,
            };
            match clause {
                Some(("catch", items))
                    if catch.is_none() && finally.is_none() && items.len() >= 3 =>
                {
                    Self::binding(&items[1])?;
                    body_end = body_end.min(index);
                    catch = Some(items.as_slice());
                }
                Some(("finally", items)) if finally.is_none() && items.len() >= 2 => {
                    body_end = body_end.min(index);
                    finally = Some(items.as_slice());
                }
                Some(_) => {
                    return Err(Error::new(
                        arg,
                        "invalid or duplicate try clause; catch precedes finally",
                    )
                    .into());
                }
                None if body_end != args.len() => {
                    return Err(Error::new(arg, "try clauses must follow its body").into());
                }
                None => {}
            }
        }
        if body_end == 0 || (catch.is_none() && finally.is_none()) {
            return Err(Error::new(expr, "try needs a body and catch or finally").into());
        }
        let local = self.child_scope(env);
        let mut result = self.sequence(&args[..body_end], &local, mode, io);
        if let (Some(items), Err(Signal::Error(error))) = (catch, &result)
            && !error.exit
            && !error.cancelled
        {
            let handler = self.child_scope(env);
            handler
                .borrow_mut()
                .variables
                .insert(Self::binding(&items[1])?.into(), error.as_value(true));
            result = self.sequence(&items[2..], &handler, mode, io);
        }
        if let Some(items) = finally {
            let cleanup = self.child_scope(env);
            self.finally_depth += 1;
            self.cleanup_depth += 1;
            let cleaned = self.sequence(&items[1..], &cleanup, Mode::Normal, io);
            self.finally_depth -= 1;
            self.cleanup_depth -= 1;
            if cleaned.is_err() && !matches!(&result, Err(Signal::Error(e)) if e.cancelled) {
                result = cleaned;
            }
        }
        result
    }
}
