use super::{Env, Error, EvalResult, Number, Shell, Value};
use crate::{reader::Expr, streams::Io};
use indexmap::IndexMap;

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
        let Value::Map(mut values) = self.value(target, env, io)? else {
            return Err(Error::new(target, "expected a map").into());
        };
        match command {
            "mget" => {
                let key = self.map_key(&args[1], env, io)?;
                if let Some(value) = values.get(&key).cloned() {
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
                values.insert(key, value);
                Ok(Value::Map(values))
            }
            "mdel" => {
                let key = self.map_key(&args[1], env, io)?;
                values
                    .shift_remove(&key)
                    .ok_or_else(|| Error::new(&args[1], format!("map key {key:?} not found")))?;
                Ok(Value::Map(values))
            }
            "mhas" => {
                let key = self.map_key(&args[1], env, io)?;
                Ok(Value::Bool(values.contains_key(&key)))
            }
            "mkeys" => Ok(Value::List(values.into_keys().map(Value::Text).collect())),
            "mvals" => Ok(Value::List(values.into_values().collect())),
            "mlen" => Ok(Value::Number(Number::Int(values.len() as i64))),
            _ => unreachable!("unknown map command"),
        }
    }
}
