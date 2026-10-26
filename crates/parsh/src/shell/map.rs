use super::{Arguments, Env, Error, EvalResult, Number, Shell, Value};
use crate::{reader::Expr, streams::Io};
use indexmap::IndexMap;

impl Shell {
    fn map_key(
        &mut self,
        expr: &Expr,
        args: Arguments<'_>,
        index: usize,
        env: &Env,
        io: &Io,
    ) -> EvalResult<String> {
        match args.value(self, index, env, io)? {
            Value::Text(key) => Ok(key),
            _ => Err(Error::new(args.at(expr, index), "map keys must be text").into()),
        }
    }

    pub(super) fn construct_map(
        &mut self,
        expr: &Expr,
        args: Arguments<'_>,
        env: &Env,
        io: &Io,
    ) -> EvalResult<Value> {
        if !args.len().is_multiple_of(2) {
            return Err(Error::new(expr, "map expects alternating keys and values").into());
        }
        let mut values = IndexMap::new();
        for index in (0..args.len()).step_by(2) {
            let key = self.map_key(expr, args, index, env, io)?;
            if values.contains_key(&key) {
                return Err(
                    Error::new(args.at(expr, index), format!("duplicate map key {key:?}")).into(),
                );
            }
            let value = args.value(self, index + 1, env, io)?;
            values.insert(key, value);
        }
        Ok(Value::Map(values))
    }

    pub(super) fn map_command(
        &mut self,
        expr: &Expr,
        command: &str,
        args: Arguments<'_>,
        env: &Env,
        io: &Io,
    ) -> EvalResult<Value> {
        let (min, max) = match command {
            "map/get" => (2, 3),
            "map/set" => (3, 3),
            "map/del" | "map/has" => (2, 2),
            "map/keys" | "map/vals" | "map/len" => (1, 1),
            _ => unreachable!("unknown map command"),
        };
        Self::arity_count(expr, args.len(), min, max)?;
        let evaluated;
        let args = if command == "map/get" {
            evaluated = args.values(self, env, io)?;
            args.with_values(&evaluated)
        } else {
            args
        };
        let target = args.at(expr, 0);
        let Value::Map(mut values) = args.value(self, 0, env, io)? else {
            return Err(Error::new(target, "expected a map").into());
        };
        match command {
            "map/get" => {
                let key = self.map_key(expr, args, 1, env, io)?;
                if let Some(value) = values.get(&key).cloned() {
                    Ok(value)
                } else if args.len() == 3 {
                    args.value(self, 2, env, io)
                } else {
                    Err(Error::new(args.at(expr, 1), format!("map key {key:?} not found")).into())
                }
            }
            "map/set" => {
                let key = self.map_key(expr, args, 1, env, io)?;
                let value = args.value(self, 2, env, io)?;
                values.insert(key, value);
                Ok(Value::Map(values))
            }
            "map/del" => {
                let key = self.map_key(expr, args, 1, env, io)?;
                values.shift_remove(&key).ok_or_else(|| {
                    Error::new(args.at(expr, 1), format!("map key {key:?} not found"))
                })?;
                Ok(Value::Map(values))
            }
            "map/has" => {
                let key = self.map_key(expr, args, 1, env, io)?;
                Ok(Value::Bool(values.contains_key(&key)))
            }
            "map/keys" => Ok(Value::List(values.into_keys().map(Value::Text).collect())),
            "map/vals" => Ok(Value::List(values.into_values().collect())),
            "map/len" => Ok(Value::Number(Number::Int(values.len() as i64))),
            _ => unreachable!("unknown map command"),
        }
    }
}
