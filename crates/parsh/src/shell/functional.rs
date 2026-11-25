use std::cmp::Ordering;

use indexmap::IndexMap;

use super::{
    Arguments, Env, Error, EvalResult, Function, FunctionValue, Mode, Number, Parameters, Shell,
    Value,
};
use crate::{
    reader::{Expr, Kind, Read, Reader},
    streams::Io,
};

pub(super) const NAMES: &[&str] = &[
    "identity",
    "constantly",
    "partial",
    "compose",
    "complement",
    "even?",
    "odd?",
    "zero?",
    "positive?",
    "negative?",
    "list/flat-map",
    "list/sort-by",
    "list/group-by",
    "list/take-while",
    "list/drop-while",
    "list/partition",
    "map/update",
    "map/get-in",
    "map/set-in",
    "map/update-in",
];

impl Shell {
    /// Adapters are ordinary lexical closures, including when transferred to a task.
    fn adapter(
        &mut self,
        name: &str,
        source: &str,
        bindings: Vec<(&str, Value)>,
        env: &Env,
    ) -> FunctionValue {
        let Read::Form(Expr {
            kind: Kind::List(parts),
            ..
        }) = Reader::new(source).read()
        else {
            unreachable!("adapter definitions are valid function forms")
        };
        let closure = self.child_scope(env);
        closure
            .borrow_mut()
            .variables
            .extend(bindings.into_iter().map(|(key, value)| (key.into(), value)));
        FunctionValue::user(
            name,
            Function {
                parameters: Parameters::parse(&parts[1], "function")
                    .expect("valid adapter parameters"),
                body: parts[2..].to_vec(),
                closure,
            },
        )
    }

    pub(super) fn partial_function(
        &mut self,
        name: &str,
        callee: Value,
        bound: Vec<Value>,
        env: &Env,
    ) -> FunctionValue {
        self.adapter(
            name,
            "(fn (&rest args) (apply $callee (list/concat $bound $args)))",
            vec![("callee", callee), ("bound", Value::List(bound))],
            env,
        )
    }

    pub(super) fn functional_builtin(
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
        match name {
            "identity" => Ok(values[0].clone()),
            "constantly" => Ok(Value::Function(self.adapter(
                name,
                "(fn (&rest ignored) $constant)",
                vec![("constant", values[0].clone())],
                env,
            ))),
            "partial" => {
                let callee = self.resolve_callable(args.at(expr, 0), values[0].clone())?;
                Ok(Value::Function(self.partial_function(
                    name,
                    callee,
                    values[1..].to_vec(),
                    env,
                )))
            }
            "compose" => {
                if values.is_empty() {
                    return Ok(Value::Function(self.function_value(expr, "identity")?));
                }
                let mut functions = Vec::with_capacity(values.len());
                for (index, value) in values.into_iter().enumerate() {
                    functions.push(self.resolve_callable(args.at(expr, index), value)?);
                }
                functions.reverse();
                Ok(Value::Function(self.adapter(
                    name,
                    r#"(fn (&rest args)
                    (let result (apply (list/first $functions) $args))
                    (for f (list/rest $functions) (set result (apply $f [$result])))
                    $result)"#,
                    vec![("functions", Value::List(functions))],
                    env,
                )))
            }
            "complement" => {
                let predicate = self.resolve_callable(args.at(expr, 0), values[0].clone())?;
                Ok(Value::Function(self.adapter(
                    name,
                    r#"(fn (&rest args)
                    (let result (apply $predicate $args))
                    (if (bool? $result) (not $result)
                        (throw "complement predicate must return bool")))"#,
                    vec![("predicate", predicate)],
                    env,
                )))
            }
            "even?" | "odd?" => {
                let integer = ready.integer(self, expr, 0, env, io)?;
                Ok(Value::Bool((integer % 2 == 0) == (name == "even?")))
            }
            "zero?" | "positive?" | "negative?" => {
                let order = ready
                    .number(self, expr, 0, env, io)?
                    .compare(Number::Int(0));
                Ok(Value::Bool(
                    order
                        == Some(match name {
                            "zero?" => Ordering::Equal,
                            "positive?" => Ordering::Greater,
                            _ => Ordering::Less,
                        }),
                ))
            }
            "map/update" | "map/get-in" | "map/set-in" | "map/update-in" => {
                self.nested_map_builtin(expr, name, ready, env, mode, io)
            }
            "list/partition" => {
                let size = ready.integer(self, expr, 0, env, io)?;
                if size <= 0 {
                    return Err(Error::new(
                        args.at(expr, 0),
                        "list/partition size must be positive",
                    )
                    .into());
                }
                let Value::List(items) = &values[1] else {
                    return Err(Error::new(args.at(expr, 1), "expected a list").into());
                };
                let size = usize::try_from(size).unwrap_or(usize::MAX);
                Ok(Value::List(
                    items
                        .chunks(size)
                        .map(|chunk| Value::List(chunk.to_vec()))
                        .collect(),
                ))
            }
            _ => self.sequence_transform(expr, name, ready, env, mode, io),
        }
    }

    fn sequence_transform(
        &mut self,
        expr: &Expr,
        name: &str,
        args: Arguments<'_>,
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        let callback = args.value(self, 0, env, io)?;
        let callback = self.resolve_callable(args.at(expr, 0), callback)?;
        let Value::List(items) = args.value(self, 1, env, io)? else {
            return Err(Error::new(args.at(expr, 1), "expected a list").into());
        };
        let mut output = Vec::new();
        let mut groups = IndexMap::<String, Value>::new();
        let mut keyed = Vec::new();
        let mut family = None;
        let mut remaining = items.into_iter();
        while let Some(item) = remaining.next() {
            let result = self.call_value(
                expr,
                callback.clone(),
                Arguments::Values(std::slice::from_ref(&item)),
                env,
                mode,
                io,
            )?;
            match name {
                "list/flat-map" => {
                    let Value::List(items) = result else {
                        return Err(Error::new(
                            args.at(expr, 0),
                            "list/flat-map callback must return a list",
                        )
                        .into());
                    };
                    output.extend(items);
                }
                "list/take-while" | "list/drop-while" => {
                    let Value::Bool(keep) = result else {
                        return Err(Error::new(
                            args.at(expr, 0),
                            format!("{name} predicate must return bool"),
                        )
                        .into());
                    };
                    if name == "list/take-while" {
                        if !keep {
                            break;
                        }
                        output.push(item);
                    } else if !keep {
                        output.push(item);
                        output.extend(remaining);
                        break;
                    }
                }
                "list/group-by" => {
                    let Value::Text(key) = result else {
                        return Err(Error::new(
                            args.at(expr, 0),
                            "list/group-by callback must return text keys",
                        )
                        .into());
                    };
                    let Value::List(group) =
                        groups.entry(key).or_insert_with(|| Value::List(Vec::new()))
                    else {
                        unreachable!()
                    };
                    group.push(item);
                }
                "list/sort-by" => {
                    let text = sort_key(args.at(expr, 0), &result)?;
                    if family.is_some_and(|family| family != text) {
                        return Err(Error::new(
                            args.at(expr, 0),
                            "sort keys must be all text or all finite numbers",
                        )
                        .into());
                    }
                    family = Some(text);
                    keyed.push((result, item));
                }
                _ => unreachable!("unknown sequence transform"),
            }
        }
        Ok(match name {
            "list/group-by" => Value::Map(groups),
            "list/sort-by" => {
                keyed.sort_by(|(left, _), (right, _)| compare_keys(left, right));
                Value::List(keyed.into_iter().map(|(_, item)| item).collect())
            }
            _ => Value::List(output),
        })
    }

    fn nested_map_builtin(
        &mut self,
        expr: &Expr,
        name: &str,
        args: Arguments<'_>,
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        let Value::Map(mut map) = args.value(self, 0, env, io)? else {
            return Err(Error::new(args.at(expr, 0), "expected a map").into());
        };
        let keys = if name == "map/update" {
            vec![args.text(self, expr, 1, env, io)?]
        } else {
            let Value::List(path) = args.value(self, 1, env, io)? else {
                return Err(Error::new(
                    args.at(expr, 1),
                    "map path must be a nonempty list of text keys",
                )
                .into());
            };
            if path.is_empty() || path.len() > 128 {
                return Err(
                    Error::new(args.at(expr, 1), "map path must contain 1..128 text keys").into(),
                );
            }
            path.into_iter()
                .map(|key| match key {
                    Value::Text(key) => Ok(key),
                    _ => Err(Error::new(
                        args.at(expr, 1),
                        "map path must contain text keys",
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let old = lookup_path(&map, &keys, args.at(expr, 1))?;
        if name == "map/get-in" {
            return match old {
                Some(value) => Ok(value),
                None if args.len() == 3 => args.value(self, 2, env, io),
                None => Err(Error::new(args.at(expr, 1), "map path not found").into()),
            };
        }
        let value = if name == "map/set-in" {
            args.value(self, 2, env, io)?
        } else {
            let callback = args.value(self, 2, env, io)?;
            let mut values = vec![old.unwrap_or(Value::Nil)];
            for index in 3..args.len() {
                values.push(args.value(self, index, env, io)?);
            }
            self.call_value(expr, callback, Arguments::Values(&values), env, mode, io)?
        };
        set_path(&mut map, &keys, value, args.at(expr, 1))?;
        Ok(Value::Map(map))
    }
}

fn lookup_path(
    map: &IndexMap<String, Value>,
    keys: &[String],
    expr: &Expr,
) -> Result<Option<Value>, Error> {
    let mut current = map;
    for (index, key) in keys.iter().enumerate() {
        let Some(value) = current.get(key) else {
            return Ok(None);
        };
        if index + 1 == keys.len() {
            return Ok(Some(value.clone()));
        }
        let Value::Map(child) = value else {
            return Err(Error::new(expr, "map path contains a non-map value"));
        };
        current = child;
    }
    unreachable!("paths are nonempty")
}

fn set_path(
    map: &mut IndexMap<String, Value>,
    keys: &[String],
    value: Value,
    expr: &Expr,
) -> Result<(), Error> {
    if keys.len() == 1 {
        map.insert(keys[0].clone(), value);
        return Ok(());
    }
    let child = map
        .entry(keys[0].clone())
        .or_insert_with(|| Value::Map(IndexMap::new()));
    let Value::Map(child) = child else {
        return Err(Error::new(expr, "map path contains a non-map value"));
    };
    set_path(child, &keys[1..], value, expr)
}

fn sort_key(expr: &Expr, value: &Value) -> Result<bool, Error> {
    match value {
        Value::Text(_) => Ok(true),
        Value::Number(Number::Int(_)) => Ok(false),
        Value::Number(Number::Float(value)) if value.is_finite() => Ok(false),
        _ => Err(Error::new(
            expr,
            "sort keys must be all text or all finite numbers",
        )),
    }
}

fn compare_keys(left: &Value, right: &Value) -> Ordering {
    match (left, right) {
        (Value::Text(left), Value::Text(right)) => left.cmp(right),
        (Value::Number(left), Value::Number(right)) => {
            left.compare(*right).expect("finite sort keys")
        }
        _ => unreachable!("sort key types were validated"),
    }
}
