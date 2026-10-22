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

pub struct CommandOutput {
    pub code: i32,
    pub chunks: Vec<OutputChunk>,
}

#[derive(Clone, Copy, Default)]
pub struct HostStreams {
    pub input: Option<u32>,
    pub output: Option<u32>,
    pub error: Option<u32>,
    pub capture: bool,
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
    fn close(&mut self, handle: u32);
    fn chdir(&mut self, cwd: &str, path: &str) -> Result<String, HostError>;
    /// Available external command names, without running any commands.
    fn commands(&mut self) -> Result<Vec<String>, HostError> {
        Ok(Vec::new())
    }
    /// Absolute paths; directories have a trailing slash.
    fn paths(&mut self) -> Result<Vec<String>, HostError>;
}
