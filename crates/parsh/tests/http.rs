#![cfg(feature = "native")]

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::Command,
    thread,
};

fn read_request(stream: &mut TcpStream) -> (String, String, Vec<(String, String)>, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut buffer = [0; 1024];
    let header_end = loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "connection closed before request headers");
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let header_text = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
    let mut lines = header_text.split("\r\n");
    let mut request_line = lines.next().unwrap().split_whitespace();
    let method = request_line.next().unwrap().to_owned();
    let path = request_line.next().unwrap().to_owned();
    let mut headers = Vec::new();
    let mut content_length = 0;
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').unwrap();
        let value = value.trim().to_owned();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse().unwrap();
        }
        headers.push((name.to_ascii_lowercase(), value));
    }
    while bytes.len() < header_end + content_length {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "connection closed before request body");
        bytes.extend_from_slice(&buffer[..count]);
    }
    (
        method,
        path,
        headers,
        bytes[header_end..header_end + content_length].to_vec(),
    )
}

fn respond(mut stream: TcpStream, status: &str, headers: &[(&str, &str)], body: &[u8]) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n",
        body.len()
    )
    .unwrap();
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").unwrap();
    }
    stream.write_all(b"Connection: close\r\n\r\n").unwrap();
    stream.write_all(body).unwrap();
}

#[test]
fn http_builtins_return_structured_responses_and_preserve_json_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server =
        thread::spawn(move || {
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().unwrap();
                let (method, path, headers, body) = read_request(&mut stream);
                match (method.as_str(), path.as_str()) {
                    ("GET", "/get") => {
                        assert!(headers.iter().any(|(name, value)| {
                            name == "accept" && value == "application/json"
                        }));
                        assert!(body.is_empty());
                        respond(
                            stream,
                            "200 OK",
                            &[
                                ("X-Test", "first"),
                                ("x-test", "second"),
                                ("Content-Type", "application/json"),
                            ],
                            br#"{"message":"ok"}"#,
                        );
                    }
                    ("POST", "/post") => {
                        assert_eq!(body, br#"{"sent":true}"#);
                        assert!(headers.iter().any(|(name, value)| {
                            name == "content-type" && value == "application/json"
                        }));
                        respond(stream, "201 Created", &[], b"created");
                    }
                    ("PUT", "/put") => {
                        assert!(body.is_empty());
                        respond(stream, "204 No Content", &[], b"");
                    }
                    ("GET", "/status") => {
                        respond(stream, "404 Not Found", &[], b"missing");
                    }
                    request => panic!("unexpected request: {request:?}"),
                }
            }
        });

    let base = format!("http://{address}");
    let source = format!(
        r#"
        (let get (http/get "{base}/get" (map "accept" ["application/json"])))
        (let post (http/post "{base}/post" "{{\"sent\":true}}" (map "content-type" ["application/json"])))
        (let put (http/request "PUT" "{base}/put" #nil {{}}))
        (let missing (http/get "{base}/status"))
        (printf "%s\n"
          (list
            (HttpResponse.status $get)
            (HttpResponse.ok $get)
            (str/join (map/get (HttpResponse.headers $get) "x-test") ",")
            (json/encode (json/decode (HttpResponse.body $get)))
            (http/body-text $get)
            (HttpResponse.status $post)
            (HttpResponse.ok $post)
            (HttpResponse.status $put)
            (HttpResponse.status $missing)
            (HttpResponse.ok $missing)
            (http/body-text $missing)))
        "#,
        base = base,
    );
    let output = Command::new(env!("CARGO_BIN_EXE_parsh"))
        .args(["-c", &source])
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "200\ntrue\nfirst,second\n{\"message\":\"ok\"}\n{\"message\":\"ok\"}\n201\ntrue\n204\n404\nfalse\nmissing\n"
    );
    server.join().unwrap();
}

#[test]
fn http_transport_errors_are_language_errors() {
    let output = Command::new(env!("CARGO_BIN_EXE_parsh"))
        .args(["-c", "(http/get \"http://127.0.0.1:1/unreachable\")"])
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("http/request:"));
}
