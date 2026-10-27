use super::{Env, Error, EvalResult, Mode, Number, Shell, Value, builtins::Arguments};
use crate::{reader::Expr, streams::Io};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::sync::Arc;

fn error(expr: &Expr, message: impl Into<String>) -> Error {
    Error {
        kind: "bytes".into(),
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
    let values = args.values(shell, env, io)?;
    match name {
        "bytes/len" => {
            let Value::Bytes(bytes) = &values[0] else {
                return Err(error(args.at(expr, 0), "bytes/len expects bytes").into());
            };
            Ok(Value::Number(Number::Int(bytes.len() as i64)))
        }
        "bytes/slice" => {
            let Value::Bytes(bytes) = &values[0] else {
                return Err(error(args.at(expr, 0), "bytes/slice expects bytes").into());
            };
            let start = index(expr, args, &values, 1, bytes.len())?;
            let end = if values.len() == 3 {
                index(expr, args, &values, 2, bytes.len())?
            } else {
                bytes.len()
            };
            Ok(Value::Bytes(Arc::from(&bytes[start..end.max(start)])))
        }
        "bytes/concat" => {
            let mut result = Vec::new();
            for (index, value) in values.iter().enumerate() {
                let Value::Bytes(bytes) = value else {
                    return Err(error(args.at(expr, index), "bytes/concat expects bytes").into());
                };
                result.extend_from_slice(bytes);
            }
            Ok(Value::Bytes(Arc::from(result)))
        }
        "bytes/from-text" => match &values[0] {
            Value::Text(text) => Ok(Value::Bytes(Arc::from(text.as_bytes()))),
            _ => Err(error(args.at(expr, 0), "bytes/from-text expects text").into()),
        },
        "bytes/to-text" => match &values[0] {
            Value::Bytes(bytes) => String::from_utf8(bytes.to_vec())
                .map(Value::Text)
                .map_err(|_| error(args.at(expr, 0), "bytes/to-text: bytes are not UTF-8").into()),
            _ => Err(error(args.at(expr, 0), "bytes/to-text expects bytes").into()),
        },
        "bytes/base64-encode" => match &values[0] {
            Value::Bytes(bytes) => Ok(Value::Text(STANDARD.encode(bytes))),
            _ => Err(error(args.at(expr, 0), "bytes/base64-encode expects bytes").into()),
        },
        "bytes/base64-decode" => match &values[0] {
            Value::Text(text) => STANDARD
                .decode(text)
                .map(|bytes| Value::Bytes(Arc::from(bytes)))
                .map_err(|message| {
                    error(args.at(expr, 0), format!("bytes/base64-decode: {message}")).into()
                }),
            _ => Err(error(args.at(expr, 0), "bytes/base64-decode expects text").into()),
        },
        "bytes/hex-encode" => match &values[0] {
            Value::Bytes(bytes) => Ok(Value::Text(hex_encode(bytes))),
            _ => Err(error(args.at(expr, 0), "bytes/hex-encode expects bytes").into()),
        },
        "bytes/hex-decode" => match &values[0] {
            Value::Text(text) => hex_decode(text)
                .map(|bytes| Value::Bytes(Arc::from(bytes)))
                .map_err(|message| {
                    error(args.at(expr, 0), format!("bytes/hex-decode: {message}")).into()
                }),
            _ => Err(error(args.at(expr, 0), "bytes/hex-decode expects text").into()),
        },
        _ => unreachable!("unknown bytes builtin"),
    }
}

fn index(
    expr: &Expr,
    args: Arguments<'_>,
    values: &[Value],
    argument: usize,
    len: usize,
) -> Result<usize, crate::shell::Signal> {
    let Value::Number(Number::Int(index)) = values.get(argument).cloned().unwrap_or(Value::Nil)
    else {
        return Err(error(args.at(expr, argument), "bytes index expects an integer").into());
    };
    let index = if index < 0 {
        len as i128 + i128::from(index)
    } else {
        i128::from(index)
    };
    Ok(index.clamp(0, len as i128) as usize)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        text.push(HEX[(byte >> 4) as usize] as char);
        text.push(HEX[(byte & 0x0f) as usize] as char);
    }
    text
}

fn hex_decode(text: &str) -> Result<Vec<u8>, String> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err("hex input must contain an even number of digits".into());
    }
    let mut result = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        let high =
            hex_digit(pair[0]).ok_or_else(|| "hex input contains an invalid digit".to_owned())?;
        let low =
            hex_digit(pair[1]).ok_or_else(|| "hex input contains an invalid digit".to_owned())?;
        result.push((high << 4) | low);
    }
    Ok(result)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
