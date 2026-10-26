pub use crate::streams::{Io, Output};
use crate::{host::Termination, job::JobControl, shell::tasks::Control, streams::append_capture};
use std::{
    collections::HashMap,
    ffi::OsString,
    io::{self, Read},
    os::unix::process::{CommandExt, ExitStatusExt},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::Duration,
};

#[derive(Clone, Debug)]
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

pub struct StageOutput {
    pub argv: Vec<String>,
    pub termination: Option<Termination>,
    pub error: Option<ProcessError>,
}
pub struct ProcessOutput {
    pub stages: Vec<StageOutput>,
    pub error: Option<ProcessError>,
}
impl ProcessOutput {
    pub fn termination(&self) -> Option<&Termination> {
        self.stages
            .iter()
            .rev()
            .filter_map(|s| s.termination.as_ref())
            .find(|s| s.code() != 0)
            .or_else(|| self.stages.last().and_then(|s| s.termination.as_ref()))
    }
}

#[derive(Default)]
struct Children {
    processes: Vec<Child>,
    group: Option<i32>,
    reaped: bool,
}
impl Children {
    fn signal(&mut self, signal: nix::sys::signal::Signal) {
        if let Some(group) = self.group {
            let _ = nix::sys::signal::killpg(nix::unistd::Pid::from_raw(group), signal);
        } else {
            for child in &self.processes {
                let _ =
                    nix::sys::signal::kill(nix::unistd::Pid::from_raw(child.id() as i32), signal);
            }
        }
    }
}
impl Drop for Children {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        self.signal(nix::sys::signal::Signal::SIGKILL);
        for child in &mut self.processes {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn status(status: std::process::ExitStatus) -> Termination {
    match status.code() {
        Some(code) => Termination::Exited(code),
        None => Termination::Signaled {
            signal: status.signal().unwrap_or(0),
            core_dumped: status.core_dumped(),
        },
    }
}

fn reader(
    mut input: impl Read + Send + 'static,
    output: Arc<Mutex<Vec<u8>>>,
    budget: Option<Arc<crate::streams::CaptureBudget>>,
) -> thread::JoinHandle<io::Result<()>> {
    thread::spawn(move || {
        let mut bytes = [0u8; 16384];
        loop {
            let count = input.read(&mut bytes)?;
            if count == 0 {
                return Ok(());
            }
            // Keep draining until the supervisor terminates the process on overflow.
            let _ = append_capture(&output, budget.as_ref(), &bytes[..count]);
        }
    })
}

pub(crate) fn run(
    stages: &[Vec<String>],
    cwd: &Path,
    environment: &HashMap<OsString, OsString>,
    io: &Io,
    job: Option<&JobControl>,
    control: Option<&Arc<Control>>,
    cleanup: bool,
) -> Result<ProcessOutput, ProcessError> {
    let _foreground = job.map(JobControl::guard).transpose()?;
    let mut children = Children::default();
    let mut previous = None;
    let mut readers = Vec::new();
    let mut group = 0;
    let mut report = ProcessOutput {
        stages: Vec::new(),
        error: None,
    };
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
            None if io.error_capture.is_some() => Stdio::piped(),
            None => Stdio::inherit(),
        });
        if let Some(job) = job {
            job.configure(&mut command, group);
        } else if control.is_some() {
            command.process_group(group);
            // SAFETY: only async-signal-safe signal() calls run before exec.
            unsafe {
                command.pre_exec(|| {
                    for signal in [
                        nix::libc::SIGINT,
                        nix::libc::SIGQUIT,
                        nix::libc::SIGPIPE,
                        nix::libc::SIGTSTP,
                        nix::libc::SIGTTIN,
                        nix::libc::SIGTTOU,
                    ] {
                        nix::libc::signal(signal, nix::libc::SIG_DFL);
                    }
                    Ok(())
                });
            }
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                drop(command);
                drop(previous.take());
                let error = ProcessError {
                    code: match error.kind() {
                        io::ErrorKind::NotFound => 127,
                        io::ErrorKind::PermissionDenied => 126,
                        _ => 2,
                    },
                    message: format!("{}: {error}", argv[0]),
                };
                if let Some(control) = control {
                    control.started(Err(error.clone()));
                }
                report.stages.push(StageOutput {
                    argv: argv.clone(),
                    termination: None,
                    error: Some(error.clone()),
                });
                report.error = Some(error);
                children.signal(nix::sys::signal::Signal::SIGKILL);
                break;
            }
        };
        if group == 0 {
            group = child.id() as i32;
            if job.is_some() || control.is_some() {
                children.group = Some(group);
            }
            if let Some(control) = control {
                control.register(group);
            }
        }
        if !last {
            previous = child.stdout.take();
        } else if let Output::Capture(buffer) = &io.output {
            readers.push(reader(
                child.stdout.take().expect("capture pipe"),
                buffer.clone(),
                io.capture_budget.clone(),
            ));
        }
        if let Some(buffer) = &io.error_capture
            && io.error.is_none()
        {
            readers.push(reader(
                child.stderr.take().expect("stderr capture pipe"),
                buffer.clone(),
                io.capture_budget.clone(),
            ));
        }
        children.processes.push(child);
        report.stages.push(StageOutput {
            argv: argv.clone(),
            termination: None,
            error: None,
        });
    }
    drop(previous);
    if report.error.is_none()
        && let Some(control) = control
    {
        control.started(Ok(()));
    }
    let mut finished = vec![false; children.processes.len()];
    let mut terminated = false;
    let mut killed = report.error.is_some();
    loop {
        let overflow = io
            .capture_budget
            .as_ref()
            .is_some_and(|b| b.exceeded.load(Ordering::Relaxed));
        let cancelling = control.is_some_and(|c| c.cancelled() && !cleanup);
        let forced = control.is_some_and(|c| c.forced());
        if overflow || forced {
            if !killed {
                children.signal(nix::sys::signal::Signal::SIGKILL);
                killed = true;
            }
            if overflow && report.error.is_none() {
                report.error = Some(ProcessError {
                    code: 2,
                    message: "command output exceeds 8 MiB".into(),
                });
            }
        } else if cancelling && !terminated {
            children.signal(nix::sys::signal::Signal::SIGTERM);
            terminated = true;
        }
        for (index, child) in children.processes.iter_mut().enumerate() {
            if finished[index] {
                continue;
            }
            match child.try_wait() {
                Ok(Some(exit)) => {
                    report.stages[index].termination = Some(status(exit));
                    finished[index] = true;
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    if let Ok(exit) = child.wait() {
                        report.stages[index].termination = Some(status(exit));
                    }
                    report.error = Some(error.into());
                    finished[index] = true;
                }
            }
        }
        if finished.iter().all(|f| *f) && readers.iter().all(thread::JoinHandle::is_finished) {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    children.reaped = true;
    for reader in readers {
        match reader.join() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => report.error = Some(error.into()),
            Err(_) => {
                report.error = Some(ProcessError {
                    code: 2,
                    message: "output reader panicked".into(),
                })
            }
        }
    }
    if let Some(control) = control {
        // Background subprocesses cannot detach descendants from their owning task.
        children.signal(nix::sys::signal::Signal::SIGKILL);
        control.unregister(group);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_status_preserves_signal_core_dump_and_normal_exit() {
        assert_eq!(
            status(std::process::ExitStatus::from_raw(143 << 8)),
            Termination::Exited(143)
        );
        assert_eq!(
            status(std::process::ExitStatus::from_raw(nix::libc::SIGTERM)),
            Termination::Signaled {
                signal: nix::libc::SIGTERM,
                core_dumped: false
            }
        );
        assert_eq!(
            status(std::process::ExitStatus::from_raw(
                nix::libc::SIGQUIT | 0x80
            )),
            Termination::Signaled {
                signal: nix::libc::SIGQUIT,
                core_dumped: true
            }
        );
    }
}
