use std::{
    collections::HashMap,
    ffi::OsString,
    io::{self, Read},
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
};

use crate::job::JobControl;

pub use crate::streams::{Io, Output};

pub struct ProcessError {
    pub code: i32,
    pub message: String,
}

impl From<io::Error> for ProcessError {
    fn from(error: io::Error) -> Self {
        Self {
            code: 2,
            message: error.to_string(),
        }
    }
}

#[derive(Default)]
struct Children {
    processes: Vec<Child>,
    group: Option<i32>,
    reaped: bool,
}

impl Drop for Children {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        if let Some(group) = self.group {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(group),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
        for child in &mut self.processes {
            let _ = child.kill();
        }
        for child in &mut self.processes {
            let _ = child.wait();
        }
    }
}

pub fn run(
    stages: &[Vec<String>],
    cwd: &Path,
    environment: &HashMap<OsString, OsString>,
    io: &Io,
    job: Option<&JobControl>,
) -> Result<i32, ProcessError> {
    let _foreground = job.map(JobControl::guard).transpose()?;
    let mut children = Children::default();
    let mut previous = None;
    let mut capture_thread = None;
    let mut group = 0;
    for (index, argv) in stages.iter().enumerate() {
        let mut command = Command::new(&argv[0]);
        command
            .args(&argv[1..])
            .current_dir(cwd)
            .env_clear()
            .envs(environment);
        command.stdin(match previous.take() {
            Some(pipe) => Stdio::from(pipe),
            None => match &io.input {
                Some(file) => Stdio::from(file.try_clone()?),
                None => Stdio::inherit(),
            },
        });
        let last = index + 1 == stages.len();
        command.stdout(if !last {
            Stdio::piped()
        } else {
            match &io.output {
                Output::Inherit => Stdio::inherit(),
                Output::File(file) => Stdio::from(file.try_clone()?),
                Output::Capture(_) => Stdio::piped(),
            }
        });
        command.stderr(match &io.error {
            Some(file) => Stdio::from(file.try_clone()?),
            None => Stdio::inherit(),
        });
        if let Some(job) = job {
            job.configure(&mut command, group);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                drop(command);
                drop(previous);
                return Err(ProcessError {
                    code: match error.kind() {
                        io::ErrorKind::NotFound => 127,
                        io::ErrorKind::PermissionDenied => 126,
                        _ => 2,
                    },
                    message: format!("{}: {error}", argv[0]),
                });
            }
        };
        if group == 0 {
            group = child.id() as i32;
            if job.is_some() {
                children.group = Some(group);
            }
        }
        if !last {
            previous = child.stdout.take();
        } else if matches!(io.output, Output::Capture(_)) {
            let mut stdout = child.stdout.take().expect("capture pipe");
            capture_thread = Some(thread::spawn(move || {
                let mut bytes = Vec::new();
                stdout.read_to_end(&mut bytes).map(|_| bytes)
            }));
        }
        children.processes.push(child);
    }
    // No parent writer may keep a pipeline alive while the children are waited.
    drop(previous);
    let mut result = 0;
    let mut wait_error = None;
    for child in &mut children.processes {
        match child.wait() {
            Ok(status) => {
                let code = status
                    .code()
                    .unwrap_or_else(|| 128 + status.signal().unwrap_or(0));
                if code != 0 {
                    result = code;
                }
            }
            Err(error) => {
                wait_error = Some(error);
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    children.reaped = true;
    if let Some(reader) = capture_thread {
        let bytes = reader
            .join()
            .map_err(|_| io::Error::other("stdout reader panicked"))??;
        if let Output::Capture(buffer) = &io.output {
            buffer.lock().unwrap().extend(bytes);
        }
    }
    if let Some(error) = wait_error {
        return Err(error.into());
    }
    Ok(result)
}
