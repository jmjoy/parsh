use super::{Env, Error, EvalResult, Mode, Shell, Value};
use crate::{reader::Expr, streams::Io};
#[cfg(feature = "native")]
use indexmap::IndexMap;

use super::builtins::Arguments;

#[cfg(feature = "native")]
use super::{Number, execution};
#[cfg(feature = "native")]
use crate::host::MAX_OUTPUT_BYTES;
#[cfg(feature = "native")]
use std::sync::Arc;

#[cfg(feature = "native")]
const REQUEST_TIMEOUT_SECONDS: u64 = 30;

#[cfg_attr(not(feature = "native"), allow(dead_code))]
#[derive(Debug)]
struct HttpRequest {
    method: String,
    url: String,
    body: Option<Vec<u8>>,
    headers: Vec<(String, String)>,
}

fn error(expr: &Expr, message: impl Into<String>) -> Error {
    Error {
        kind: "http".into(),
        ..Error::new(expr, message)
    }
}

fn unsupported(expr: &Expr, name: &str) -> Error {
    Error {
        kind: "unsupported".into(),
        ..Error::new(expr, format!("{name} requires native HTTP support"))
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
    if name == "http/body-text" {
        let value = args.value(shell, 0, env, io)?;
        return body_text(expr, args, value);
    }

    let values = args.values(shell, env, io)?;
    let request = match name {
        "http/request" => request_from_values(expr, args, &values, 0, 1, Some(2), Some(3))?,
        "http/get" => request_from_values(expr, args, &values, usize::MAX, 0, None, Some(1))?,
        "http/post" => request_from_values(expr, args, &values, usize::MAX, 0, Some(1), Some(2))?,
        _ => unreachable!("unknown HTTP builtin"),
    };

    #[cfg(feature = "native")]
    if !shell.syntax_only && shell.host.is_none() {
        return perform_native(expr, request);
    }
    #[cfg(not(feature = "native"))]
    let _ = (shell, request);
    Err(unsupported(expr, name).into())
}

fn request_from_values(
    expr: &Expr,
    args: Arguments<'_>,
    values: &[Value],
    method_index: usize,
    url_index: usize,
    body_index: Option<usize>,
    headers_index: Option<usize>,
) -> Result<HttpRequest, crate::shell::Signal> {
    let method = if method_index == usize::MAX {
        if body_index == Some(1) {
            "POST".to_owned()
        } else {
            "GET".to_owned()
        }
    } else {
        match values.get(method_index) {
            Some(Value::Text(method)) if !method.is_empty() => method.clone(),
            _ => {
                return Err(error(
                    args.at(expr, method_index),
                    "HTTP method must be nonempty text",
                )
                .into());
            }
        }
    };

    let url = match values.get(url_index) {
        Some(Value::Text(url)) if !url.is_empty() => url.clone(),
        _ => return Err(error(args.at(expr, url_index), "HTTP URL must be nonempty text").into()),
    };

    let body = if let Some(index) = body_index {
        match values.get(index) {
            Some(Value::Nil) => None,
            Some(Value::Text(text)) => Some(text.as_bytes().to_vec()),
            Some(Value::Bytes(bytes)) => Some(bytes.to_vec()),
            Some(_) => {
                return Err(error(
                    args.at(expr, index),
                    "HTTP body expects nil, text, or bytes",
                )
                .into());
            }
            None => None,
        }
    } else {
        None
    };

    let headers = if let Some(index) = headers_index {
        if let Some(value) = values.get(index) {
            request_headers(expr, args, index, value)?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    Ok(HttpRequest {
        method,
        url,
        body,
        headers,
    })
}

fn request_headers(
    expr: &Expr,
    args: Arguments<'_>,
    index: usize,
    value: &Value,
) -> Result<Vec<(String, String)>, crate::shell::Signal> {
    let Value::Map(entries) = value else {
        return Err(error(
            args.at(expr, index),
            "HTTP headers expects a map of text lists",
        )
        .into());
    };
    let mut headers = Vec::new();
    for (name, values) in entries {
        let Value::List(values) = values else {
            return Err(error(
                args.at(expr, index),
                "HTTP header values must be lists of text",
            )
            .into());
        };
        for value in values {
            let Value::Text(value) = value else {
                return Err(error(
                    args.at(expr, index),
                    "HTTP header values must be lists of text",
                )
                .into());
            };
            headers.push((name.clone(), value.clone()));
        }
    }
    Ok(headers)
}

fn body_text(expr: &Expr, args: Arguments<'_>, value: Value) -> EvalResult<Value> {
    let Value::Struct { name, fields } = value else {
        return Err(error(args.at(expr, 0), "http/body-text expects an HttpResponse").into());
    };
    if name != "HttpResponse" {
        return Err(error(args.at(expr, 0), "http/body-text expects an HttpResponse").into());
    }
    let Some(Value::Bytes(bytes)) = fields.get("body") else {
        return Err(error(args.at(expr, 0), "HttpResponse has no bytes body").into());
    };
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| error(args.at(expr, 0), "http/body-text: body is not UTF-8"))?;
    Ok(Value::Text(text))
}

#[cfg(feature = "native")]
fn perform_native(expr: &Expr, request: HttpRequest) -> EvalResult<Value> {
    use std::time::Duration;
    use ureq::ResponseExt;

    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(REQUEST_TIMEOUT_SECONDS)))
        .build();
    let agent = config.new_agent();
    let method = ureq::http::Method::from_bytes(request.method.as_bytes())
        .map_err(|message| error(expr, format!("http/request: invalid method: {message}")))?;
    let mut builder = ureq::http::Request::builder()
        .method(method)
        .uri(&request.url);
    {
        let headers = builder
            .headers_mut()
            .ok_or_else(|| error(expr, "http/request: unable to create headers"))?;
        for (name, value) in request.headers {
            let name =
                ureq::http::header::HeaderName::try_from(name.as_str()).map_err(|message| {
                    error(
                        expr,
                        format!("http/request: invalid header name: {message}"),
                    )
                })?;
            let value =
                ureq::http::header::HeaderValue::try_from(value.as_str()).map_err(|message| {
                    error(
                        expr,
                        format!("http/request: invalid header value: {message}"),
                    )
                })?;
            headers.append(name, value);
        }
    }
    let body = request.body.unwrap_or_default();
    let request = builder
        .body(body)
        .map_err(|message| error(expr, format!("http/request: {message}")))?;
    let mut response = agent
        .run(request)
        .map_err(|message| error(expr, format!("http/request: {message}")))?;
    let status = response.status().as_u16();
    let mut headers = IndexMap::<String, Vec<String>>::new();
    for (name, value) in response.headers() {
        let value = value
            .to_str()
            .map_err(|_| error(expr, "http/request: response header is not UTF-8"))?;
        headers
            .entry(name.as_str().to_ascii_lowercase())
            .or_default()
            .push(value.to_owned());
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_OUTPUT_BYTES as u64)
        .read_to_vec()
        .map_err(|message| error(expr, format!("http/request: {message}")))?;
    Ok(execution::record(
        "HttpResponse",
        [
            ("status", Value::Number(Number::Int(i64::from(status)))),
            ("ok", Value::Bool((200..=299).contains(&status))),
            (
                "headers",
                Value::Map(
                    headers
                        .into_iter()
                        .map(|(key, values)| {
                            (
                                key,
                                Value::List(values.into_iter().map(Value::Text).collect()),
                            )
                        })
                        .collect(),
                ),
            ),
            ("body", Value::Bytes(Arc::from(body))),
            ("url", Value::Text(response.get_uri().to_string())),
        ],
    ))
}
