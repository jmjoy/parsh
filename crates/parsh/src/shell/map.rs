use super::{Env, Error, EvalResult, Number, Scope, Shell, Value};
use crate::{reader::Expr, streams::Io};
use indexmap::IndexMap;

impl Scope {
    /// The callback must not evaluate expressions while the binding is borrowed.
    fn with_map<T>(
        env: &Env,
        target: &Expr,
        name: &str,
        operation: impl FnOnce(&mut IndexMap<String, Value>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut scope = env.borrow_mut();
        match scope.variables.get_mut(name) {
            Some(Value::Map(values)) => operation(values),
            Some(_) => Err(Error::new(
                target,
                format!("variable {name} must contain a map"),
            )),
            None => {
                let parent = scope.parent.clone();
                drop(scope);
                match parent {
                    Some(parent) => Self::with_map(&parent, target, name, operation),
                    None => Err(Error::new(target, format!("undefined variable {name}"))),
                }
            }
        }
    }
}

impl Shell {
    fn map_key(&mut self, expr: &Expr, env: &Env, io: &Io) -> EvalResult<String> {
        match self.value(expr, env, io)? {
            Value::Text(key) => Ok(key),
            _ => Err(Error::new(expr, "map keys must be text").into()),
        }
    }

    pub(super) fn construct_map(
        &mut self,
        expr: &Expr,
        args: &[Expr],
        env: &Env,
        io: &Io,
    ) -> EvalResult<Value> {
        let (pairs, remainder) = args.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err(Error::new(expr, "map expects alternating keys and values").into());
        }
        let mut values = IndexMap::new();
        for [key_expr, value_expr] in pairs {
            let key = self.map_key(key_expr, env, io)?;
            if values.contains_key(&key) {
                return Err(Error::new(key_expr, format!("duplicate map key {key:?}")).into());
            }
            let value = self.value(value_expr, env, io)?;
            values.insert(key, value);
        }
        Ok(Value::Map(values))
    }

    pub(super) fn map_command(
        &mut self,
        expr: &Expr,
        command: &str,
        args: &[Expr],
        env: &Env,
        io: &Io,
    ) -> EvalResult<Value> {
        let (min, max) = match command {
            "mget" => (2, 3),
            "mset" => (3, 3),
            "mdel" | "mhas" => (2, 2),
            "mkeys" | "mvals" | "mlen" => (1, 1),
            _ => unreachable!("unknown map command"),
        };
        Self::arity(expr, args, min, max)?;
        let target = &args[0];
        let name = Self::binding(target)?;
        match command {
            "mget" => {
                let key = self.map_key(&args[1], env, io)?;
                let value =
                    Scope::with_map(env, target, name, |values| Ok(values.get(&key).cloned()))?;
                if let Some(value) = value {
                    Ok(value)
                } else if let Some(default) = args.get(2) {
                    self.value(default, env, io)
                } else {
                    Err(Error::new(&args[1], format!("map key {key:?} not found")).into())
                }
            }
            "mset" => {
                let key = self.map_key(&args[1], env, io)?;
                let value = self.value(&args[2], env, io)?;
                Scope::with_map(env, target, name, |values| {
                    values.insert(key, value);
                    Ok(Value::Nil)
                })
                .map_err(Into::into)
            }
            "mdel" => {
                let key = self.map_key(&args[1], env, io)?;
                Scope::with_map(env, target, name, |values| {
                    values.shift_remove(&key).ok_or_else(|| {
                        Error::new(&args[1], format!("map key {key:?} not found"))
                    })?;
                    Ok(Value::Nil)
                })
                .map_err(Into::into)
            }
            "mhas" => {
                let key = self.map_key(&args[1], env, io)?;
                Scope::with_map(env, target, name, |values| {
                    Ok(Value::Bool(values.contains_key(&key)))
                })
                .map_err(Into::into)
            }
            "mkeys" | "mvals" | "mlen" => Scope::with_map(env, target, name, |values| {
                Ok(match command {
                    "mkeys" => Value::List(values.keys().cloned().map(Value::Text).collect()),
                    "mvals" => Value::List(values.values().cloned().collect()),
                    "mlen" => Value::Number(Number::Int(values.len() as i64)),
                    _ => unreachable!("unknown map command"),
                })
            })
            .map_err(Into::into),
            _ => unreachable!("unknown map command"),
        }
    }
}
