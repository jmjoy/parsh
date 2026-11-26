use super::{Arguments, Env, Error, EvalResult, FunctionValue, Shell, Value, builtin};
use crate::{
    reader::{Expr, Kind},
    streams::Io,
};

pub(super) struct Alias {
    pub(super) function: FunctionValue,
    pub(super) definition: Expr,
}

impl Shell {
    pub(super) fn alias_builtin(
        &mut self,
        expr: &Expr,
        args: &[Expr],
        env: &Env,
        io: &Io,
    ) -> EvalResult<Value> {
        if args.is_empty() {
            let mut names: Vec<_> = self.aliases.iter().collect();
            names.sort_unstable_by_key(|(name, _)| *name);
            return Ok(Value::Map(
                names
                    .into_iter()
                    .map(|(name, alias)| {
                        (
                            name.clone(),
                            Value::Form(Box::new(alias.definition.clone())),
                        )
                    })
                    .collect(),
            ));
        }
        Self::arity(expr, args, 2, 2)?;
        let name = alias_name(&args[0])?;
        if let Some(kind) = self.alias_name_conflict(name) {
            return Err(Error::new(&args[0], format!("{name} is already a {kind}")).into());
        }

        let definition = &args[1];
        let Kind::List(call) = &definition.kind else {
            return Err(Error::new(definition, "alias expects a single command call").into());
        };
        let Some((head, arguments)) = call.split_first() else {
            return Err(Error::new(definition, "alias expects a nonempty command call").into());
        };
        let target = match self.value(head, env, io)? {
            Value::Text(target) if target == name => Value::Command(target),
            target => self.resolve_callable(head, target)?,
        };
        if matches!(target, Value::Text(_)) {
            return Err(Error::new(head, "alias target must be an ordinary command").into());
        }
        if let Value::Command(name) = &target
            && (name.is_empty() || name.contains('\0'))
        {
            return Err(Error::new(head, "invalid external command name").into());
        }

        let values = Arguments::Expressions(arguments).values(self, env, io)?;
        // The target or one of its arguments may itself define a named command.
        if let Some(kind) = self.alias_name_conflict(name) {
            return Err(Error::new(&args[0], format!("{name} is already a {kind}")).into());
        }
        let mut external = match &target {
            Value::Command(name) => Some(vec![name.clone()]),
            Value::Function(function) => function.external_argv().map(<[String]>::to_vec),
            _ => None,
        };
        if let Some(prefix) = &mut external {
            Self::append_argv_values(expr, prefix, values.iter().cloned())?;
            if prefix.iter().any(|argument| argument.contains('\0')) {
                return Err(
                    Error::new(definition, "external alias arguments cannot contain NUL").into(),
                );
            }
        }
        let function = match external {
            Some(prefix) => FunctionValue::external_alias(name, prefix),
            None => self.partial_function(definition, name, target, values, Some(arguments), env),
        };
        self.aliases.insert(
            name.to_owned(),
            Alias {
                function,
                definition: definition.clone(),
            },
        );
        Ok(Value::Nil)
    }

    pub(super) fn unalias_builtin(&mut self, expr: &Expr, args: &[Expr]) -> EvalResult<Value> {
        Self::arity(expr, args, 1, 1)?;
        let name = alias_name(&args[0])?;
        if self.aliases.remove(name).is_none() {
            return Err(Error::new(&args[0], format!("no alias named {name}")).into());
        }
        Ok(Value::Nil)
    }

    fn alias_name_conflict(&self, name: &str) -> Option<&'static str> {
        if self.functions.contains_key(name) {
            Some("function")
        } else if self.macros.contains_key(name) {
            Some("macro")
        } else if self.struct_member_kind(name).is_some() {
            Some("struct command")
        } else {
            None
        }
    }
}

fn alias_name(expr: &Expr) -> Result<&str, Error> {
    let Kind::Word(name) = &expr.kind else {
        return Err(Error::new(expr, "alias name must be a bare command name"));
    };
    if name == ";" || name.is_empty() || builtin(name) {
        return Err(Error::new(expr, "alias name is reserved"));
    }
    Ok(name)
}
