use super::{Env, Error, EvalResult, Mode, Number, Shell, Value};
use crate::{reader::Expr, streams::Io};
use indexmap::IndexMap;
use serde_json::{Map as JsonMap, Value as JsonValue};

use super::builtins::Arguments;

fn error(expr: &Expr, message: impl Into<String>) -> Error {
    Error {
        kind: "json".into(),
        ..Error::new(expr, message)
    }
}

pub(super) fn call(
    shell: &mut Shell,
    expr: &Expr,
    name: &str,
    args: Arguments<'_>,
    env: &Env,
    _mode: Mode,
    io: &Io,
) -> EvalResult<Value> {
    let value = args.value(shell, 0, env, io)?;
    match name {
        "json/encode" => {
            let json = to_json(&value).map_err(|message| error(args.at(expr, 0), message))?;
            let text = serde_json::to_string(&json)
                .map_err(|message| error(args.at(expr, 0), format!("json/encode: {message}")))?;
            Ok(Value::Text(text))
        }
        "json/decode" => {
            let json = match value {
                Value::Text(text) => serde_json::from_str(&text),
                Value::Bytes(bytes) => serde_json::from_slice(&bytes),
                _ => {
                    return Err(error(args.at(expr, 0), "json/decode expects text or bytes").into());
                }
            }
            .map_err(|message| error(args.at(expr, 0), format!("json/decode: {message}")))?;
            from_json(json).map_err(|message| error(args.at(expr, 0), message).into())
        }
        _ => unreachable!("unknown JSON builtin"),
    }
}

fn to_json(value: &Value) -> Result<JsonValue, String> {
    match value {
        Value::Text(text) => Ok(JsonValue::String(text.clone())),
        Value::Number(Number::Int(number)) => Ok(JsonValue::Number((*number).into())),
        Value::Number(Number::Float(number)) => serde_json::Number::from_f64(*number)
            .map(JsonValue::Number)
            .ok_or_else(|| "json/encode cannot encode a non-finite number".into()),
        Value::Bool(value) => Ok(JsonValue::Bool(*value)),
        Value::List(values) => values
            .iter()
            .map(to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        Value::Map(values) => values
            .iter()
            .map(|(key, value)| to_json(value).map(|value| (key.clone(), value)))
            .collect::<Result<JsonMap<_, _>, _>>()
            .map(JsonValue::Object),
        Value::Struct { fields, .. } => fields
            .iter()
            .map(|(key, value)| to_json(value).map(|value| (key.clone(), value)))
            .collect::<Result<JsonMap<_, _>, _>>()
            .map(JsonValue::Object),
        Value::Nil => Ok(JsonValue::Null),
        Value::Form(_) => Err("json/encode does not support forms".into()),
        Value::Function(_) => Err("json/encode does not support functions".into()),
        Value::Command(_) => Err("json/encode does not support commands".into()),
        Value::Bytes(_) => Err("json/encode does not support bytes".into()),
        Value::Status(_) => Err("json/encode does not support command statuses".into()),
    }
}

fn from_json(value: JsonValue) -> Result<Value, String> {
    match value {
        JsonValue::Null => Ok(Value::Nil),
        JsonValue::Bool(value) => Ok(Value::Bool(value)),
        JsonValue::String(value) => Ok(Value::Text(value)),
        JsonValue::Number(number) => {
            if let Some(value) = number.as_i64() {
                return Ok(Value::Number(Number::Int(value)));
            }
            let value = number
                .as_f64()
                .ok_or_else(|| "json/decode number is outside the supported range".to_owned())?;
            Ok(Value::Number(Number::finite(value).map_err(str::to_owned)?))
        }
        JsonValue::Array(values) => values
            .into_iter()
            .map(from_json)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::List),
        JsonValue::Object(values) => values
            .into_iter()
            .map(|(key, value)| from_json(value).map(|value| (key, value)))
            .collect::<Result<IndexMap<_, _>, _>>()
            .map(Value::Map),
    }
}
