use parsh::host::{
    CommandOutput, ExecutionHost, FileMode, HostError, HostStreams, OutputChunk, Stream,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ffi::OsString};
use wasm_bindgen::JsValue;

pub struct BrowserHost(pub js_sys::Function);

#[derive(Serialize)]
struct Run<'a> {
    stages: &'a [Vec<String>],
    cwd: &'a str,
    environment: HashMap<String, String>,
    input: Option<u32>,
    output: Option<u32>,
    error: Option<u32>,
    capture: bool,
}

#[derive(Deserialize)]
struct Response<T> {
    value: Option<T>,
    error: Option<String>,
    #[serde(default = "default_code")]
    code: i32,
}
fn default_code() -> i32 {
    2
}

#[derive(Deserialize)]
struct Chunk {
    stream: String,
    bytes: Vec<u8>,
}
#[derive(Deserialize)]
struct Output {
    code: i32,
    chunks: Vec<Chunk>,
}

impl BrowserHost {
    fn call<T: for<'de> Deserialize<'de>>(
        &self,
        op: &str,
        args: &impl Serialize,
    ) -> Result<T, HostError> {
        let invoke = || -> Result<Response<T>, String> {
            let args = serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())?;
            let result = self
                .0
                .call2(&JsValue::NULL, &JsValue::from_str(op), &args)
                .map_err(|e| format!("host: {e:?}"))?;
            serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
        };
        let response = invoke().map_err(|message| HostError { code: 2, message })?;
        response.value.ok_or_else(|| HostError {
            code: response.code,
            message: response
                .error
                .unwrap_or_else(|| "invalid host response".into()),
        })
    }
}

impl ExecutionHost for BrowserHost {
    fn reset(&mut self) -> Result<String, HostError> {
        self.call("reset", &())
    }
    fn run(
        &mut self,
        stages: &[Vec<String>],
        cwd: &str,
        environment: &HashMap<OsString, OsString>,
        io: HostStreams,
    ) -> Result<CommandOutput, HostError> {
        let result: Output = self.call(
            "run",
            &Run {
                stages,
                cwd,
                environment: environment
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.to_string_lossy().into_owned(),
                            v.to_string_lossy().into_owned(),
                        )
                    })
                    .collect(),
                input: io.input,
                output: io.output,
                error: io.error,
                capture: io.capture,
            },
        )?;
        Ok(CommandOutput {
            code: result.code,
            chunks: result
                .chunks
                .into_iter()
                .map(|chunk| OutputChunk {
                    stream: if chunk.stream == "stdout" {
                        Stream::Stdout
                    } else {
                        Stream::Stderr
                    },
                    bytes: chunk.bytes,
                })
                .collect(),
        })
    }
    fn open(&mut self, cwd: &str, path: &str, mode: FileMode) -> Result<u32, HostError> {
        self.call(
            "open",
            &(
                cwd,
                path,
                match mode {
                    FileMode::Read => "read",
                    FileMode::Write => "write",
                    FileMode::Append => "append",
                },
            ),
        )
    }
    fn close(&mut self, handle: u32) {
        let _: Result<bool, _> = self.call("close", &handle);
    }
    fn write(&mut self, handle: u32, bytes: &[u8]) -> Result<(), HostError> {
        self.call::<bool>("write", &(handle, bytes)).map(|_| ())
    }
    fn chdir(&mut self, cwd: &str, path: &str) -> Result<String, HostError> {
        self.call("chdir", &(cwd, path))
    }
    fn commands(&mut self) -> Result<Vec<String>, HostError> {
        self.call("commands", &())
    }
    fn paths(&mut self) -> Result<Vec<String>, HostError> {
        self.call("paths", &())
    }
}
