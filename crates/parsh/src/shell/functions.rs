use std::{collections::HashSet, fmt, rc::Rc};

use super::{
    Arguments, Env, Error, EvalResult, Function, Mode, Scope, Shell, Signal, Value,
    builtins::VALUE_BUILTINS,
};
use crate::{
    reader::{Expr, Kind},
    streams::Io,
};

/// An opaque callable value. Copies retain its identity and lexical environment.
#[derive(Clone)]
pub struct FunctionValue(pub(super) Rc<Callable>);

pub(super) enum Callable {
    User { name: String, definition: Function },
    Builtin(String),
}

impl PartialEq for FunctionValue {
    fn eq(&self, other: &Self) -> bool {
        match (&*self.0, &*other.0) {
            (Callable::Builtin(a), Callable::Builtin(b)) => a == b,
            _ => Rc::ptr_eq(&self.0, &other.0),
        }
    }
}

impl fmt::Debug for FunctionValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display())
    }
}

impl FunctionValue {
    pub(super) fn user(name: &str, definition: Function) -> Self {
        Self(Rc::new(Callable::User {
            name: name.into(),
            definition,
        }))
    }

    pub(super) fn display(&self) -> String {
        let name = match &*self.0 {
            Callable::User { name, .. } | Callable::Builtin(name) => name,
        };
        format!("<function {name}>")
    }
}

#[derive(Clone, Default)]
pub(super) struct Parameters {
    required: Vec<String>,
    optional: Vec<(String, Option<Expr>)>,
    rest: Option<String>,
}

impl Parameters {
    pub(super) fn parse(expr: &Expr, label: &str) -> Result<Self, Error> {
        let Kind::List(items) = &expr.kind else {
            return Err(Error::new(
                expr,
                format!("{label} parameters must be a list of bare variable names"),
            ));
        };
        let mut result = Self::default();
        let mut seen = HashSet::new();
        let mut optional = false;
        let mut index = 0;
        while index < items.len() {
            let item = &items[index];
            if let Kind::Word(marker) = &item.kind {
                if marker == "&optional" {
                    if optional {
                        return Err(Error::new(item, "duplicate &optional marker"));
                    }
                    optional = true;
                    index += 1;
                    continue;
                }
                if marker == "&rest" {
                    if index + 2 != items.len() {
                        return Err(Error::new(
                            item,
                            "&rest must be followed by one final parameter name",
                        ));
                    }
                    let name = Shell::binding(&items[index + 1])?.to_owned();
                    if !seen.insert(name.clone()) {
                        return Err(Error::new(
                            &items[index + 1],
                            format!("duplicate parameter {name}"),
                        ));
                    }
                    result.rest = Some(name);
                    break;
                }
            }
            let (target, default) = if optional {
                if let Kind::List(pair) = &item.kind {
                    if pair.len() != 2 {
                        return Err(Error::new(
                            item,
                            "an optional parameter must be a name or (name default)",
                        ));
                    }
                    (&pair[0], Some(pair[1].clone()))
                } else {
                    (item, None)
                }
            } else {
                (item, None)
            };
            let name = Shell::binding(target)?.to_owned();
            if !seen.insert(name.clone()) {
                return Err(Error::new(target, format!("duplicate parameter {name}")));
            }
            if optional {
                result.optional.push((name, default));
            } else {
                result.required.push(name);
            }
            index += 1;
        }
        Ok(result)
    }

    pub(super) fn check_arity(&self, expr: &Expr, count: usize) -> Result<(), Error> {
        Shell::arity_count(
            expr,
            count,
            self.required.len(),
            if self.rest.is_some() {
                usize::MAX
            } else {
                self.required.len() + self.optional.len()
            },
        )
    }
}

impl Shell {
    pub(super) fn child_scope(&mut self, parent: &Env) -> Env {
        if self.scopes.len() >= 1024 && self.scopes.len().is_multiple_of(1024) {
            self.scopes.retain(|scope| scope.strong_count() != 0);
        }
        let scope = Scope::child(parent);
        self.scopes.push(Rc::downgrade(&scope));
        scope
    }

    pub(super) fn function_value(&self, expr: &Expr, name: &str) -> Result<FunctionValue, Error> {
        if let Some(function) = self.functions.get(name) {
            return Ok(function.clone());
        }
        if VALUE_BUILTINS.contains(&name) || matches!(name, "glob" | "cd" | "type") {
            return Ok(FunctionValue(Rc::new(Callable::Builtin(name.into()))));
        }
        Err(Error::new(
            expr,
            format!("{name} is not a function; expected a user function or ordinary builtin"),
        ))
    }

    pub(super) fn call_function(
        &mut self,
        expr: &Expr,
        function: &FunctionValue,
        args: Arguments<'_>,
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        match &*function.0 {
            Callable::User { name, definition } => {
                definition.parameters.check_arity(expr, args.len())?;
                let values = args.values(self, env, io)?;
                let positions = Self::capture_positions(io);
                let mut result = self.function_body(expr, name, definition, values, mode, io);
                self.publish_invocation(expr, &mut result, io, positions);
                result
            }
            Callable::Builtin(name) if VALUE_BUILTINS.contains(&name.as_str()) => {
                self.call_builtin(expr, name, args, env, mode, io)
            }
            Callable::Builtin(name) => {
                if name != "type" {
                    Shell::arity_count(expr, args.len(), usize::from(name == "glob"), 1)?;
                }
                if let Arguments::Expressions(expressions) = args {
                    return self.eval_named(expr, name, expressions, env, mode, io);
                }
                // Host-facing builtins consume scalar command arguments. Literal text
                // nodes preserve argument boundaries without parsing or interpolation.
                let mut expressions = Vec::new();
                let values = args.values(self, env, io)?;
                for value in values {
                    if name == "type"
                        && let Value::List(items) = value
                    {
                        for item in items {
                            expressions.push(Expr {
                                kind: Kind::Text(item.text().map_err(|m| Error::new(expr, m))?),
                                offset: expr.offset,
                            });
                        }
                        continue;
                    }
                    expressions.push(Expr {
                        kind: Kind::Text(value.text().map_err(|m| Error::new(expr, m))?),
                        offset: expr.offset,
                    });
                }
                self.eval_named(expr, name, &expressions, env, mode, io)
            }
        }
    }

    pub(super) fn function_body(
        &mut self,
        expr: &Expr,
        name: &str,
        definition: &Function,
        values: Vec<Value>,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        definition.parameters.check_arity(expr, values.len())?;
        let local = self.child_scope(&definition.closure);
        let caller_loop_depth = std::mem::take(&mut self.loop_depth);
        let caller_finally_depth = std::mem::take(&mut self.finally_depth);
        self.return_depth += 1;
        let result = (|| {
            let mut supplied = values.into_iter();
            for parameter in &definition.parameters.required {
                local
                    .borrow_mut()
                    .variables
                    .insert(parameter.clone(), supplied.next().unwrap());
            }
            for (parameter, default) in &definition.parameters.optional {
                let value = if let Some(value) = supplied.next() {
                    value
                } else if let Some(default) = default {
                    self.value(default, &local, io)?
                } else {
                    Value::Nil
                };
                local
                    .borrow_mut()
                    .variables
                    .insert(parameter.clone(), value);
            }
            if let Some(parameter) = &definition.parameters.rest {
                local
                    .borrow_mut()
                    .variables
                    .insert(parameter.clone(), Value::List(supplied.collect()));
            }
            self.sequence(&definition.body, &local, mode, io)
        })();
        self.return_depth -= 1;
        self.loop_depth = caller_loop_depth;
        self.finally_depth = caller_finally_depth;
        match result {
            Ok(value) | Err(Signal::Return(value)) => Ok(value),
            Err(signal) => {
                let mut error = signal.into_error(expr);
                if !error.exit {
                    error.message = format!("in {name}: {}", error.message);
                    error.offset = expr.offset;
                }
                Err(error.into())
            }
        }
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        #[cfg(feature = "native")]
        if self.task_id.is_none() {
            self.tasks.shutdown();
        }
        // A recursive closure can be stored in its own captured scope. Release
        // bindings explicitly so session reset/drop also releases those cycles.
        for scope in &self.scopes {
            if let Some(scope) = scope.upgrade() {
                let mut scope = scope.borrow_mut();
                scope.variables.clear();
                scope.parent = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resetting_a_session_releases_recursive_closure_scopes() {
        let mut shell = Shell::syntax();
        assert_eq!(
            crate::run_source(
                &mut shell,
                "test",
                r#"
            (let recurse (fn () $recurse))
            (defn make () (let local (fn () $local)) $local)
            (let saved (make))
        "#
            ),
            0
        );
        let scopes = shell.scopes.clone();
        shell.reset_session().unwrap();
        assert!(scopes.iter().all(|scope| scope.upgrade().is_none()));
    }
}
