//! Synchronous execution and filesystem services supplied by an isolated host.
use std::{collections::HashMap, ffi::OsString};

pub const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug)]
pub struct OutputChunk {
    pub stream: Stream,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Termination {
    Exited(i32),
    Signaled { signal: i32, core_dumped: bool },
}

impl Termination {
    pub fn code(&self) -> i32 {
        match self {
            Self::Exited(code) => *code,
            Self::Signaled { signal, .. } => 128 + signal,
        }
    }
}

#[derive(Debug)]
pub struct CommandOutput {
    pub code: i32,
    pub chunks: Vec<OutputChunk>,
    pub stages: Vec<Termination>,
    /// Execution errors may retain partial output and completed pipeline stages.
    pub error: Option<HostError>,
}

#[derive(Clone, Copy, Default)]
pub struct HostStreams {
    pub input: Option<u32>,
    pub output: Option<u32>,
    pub error: Option<u32>,
    pub capture: bool,
    pub capture_error: bool,
}

#[derive(Clone, Copy)]
pub enum FileMode {
    Read,
    Write,
    Append,
}

#[derive(Debug)]
pub struct HostError {
    pub code: i32,
    pub message: String,
}

/// No host parses parsh source: it receives evaluated argv and explicit streams.
pub trait ExecutionHost {
    /// Recreate the initial filesystem and return its working directory.
    fn reset(&mut self) -> Result<String, HostError>;
    fn run(
        &mut self,
        stages: &[Vec<String>],
        cwd: &str,
        environment: &HashMap<OsString, OsString>,
        streams: HostStreams,
    ) -> Result<CommandOutput, HostError>;
    fn open(&mut self, cwd: &str, path: &str, mode: FileMode) -> Result<u32, HostError>;
    /// Write builtin output to an open redirection handle.
    fn write(&mut self, _handle: u32, _bytes: &[u8]) -> Result<(), HostError> {
        Err(HostError {
            code: 2,
            message: "host does not support writing builtin output to files".into(),
        })
    }
    fn close(&mut self, handle: u32);
    fn chdir(&mut self, cwd: &str, path: &str) -> Result<String, HostError>;
    /// Available external command names, without running any commands.
    fn commands(&mut self) -> Result<Vec<String>, HostError> {
        Ok(Vec::new())
    }
    /// Absolute paths; directories have a trailing slash.
    fn paths(&mut self) -> Result<Vec<String>, HostError>;
}
