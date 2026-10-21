use std::{
    fs::File,
    io,
    os::fd::AsRawFd,
    os::unix::process::CommandExt,
    process::Command,
    sync::{Arc, atomic::AtomicBool},
};

use nix::{
    sys::{
        signal::{self, SigHandler, Signal},
        termios::{self, SetArg, Termios},
    },
    unistd::{self, Pid},
};

pub struct JobControl {
    terminal: File,
    shell_group: Pid,
}

impl JobControl {
    pub fn new(interrupted: Arc<AtomicBool>) -> io::Result<Self> {
        let terminal = File::options().read(true).write(true).open("/dev/tty")?;
        let group = unistd::getpgrp();
        if unistd::tcgetpgrp(&terminal)? != group {
            return Err(io::Error::other(
                "parsh must be started in the terminal foreground",
            ));
        }
        // The shell must survive terminal handoffs and foreground interrupts.
        for sig in [
            Signal::SIGQUIT,
            Signal::SIGTSTP,
            Signal::SIGTTIN,
            Signal::SIGTTOU,
        ] {
            // SAFETY: installing SIG_IGN has no Rust handler or shared state.
            unsafe {
                signal::signal(sig, SigHandler::SigIgn)?;
            }
        }
        signal_hook::flag::register(signal_hook::consts::SIGINT, interrupted)?;
        let pid = unistd::getpid();
        if group != pid {
            unistd::setpgid(Pid::from_raw(0), pid)?;
        }
        let shell_group = unistd::getpgrp();
        unistd::tcsetpgrp(&terminal, shell_group)?;
        Ok(Self {
            terminal,
            shell_group,
        })
    }

    pub fn guard(&self) -> io::Result<ForegroundGuard<'_>> {
        Ok(ForegroundGuard {
            job: self,
            saved: termios::tcgetattr(&self.terminal)?,
        })
    }

    pub fn configure(&self, command: &mut Command, group: i32) {
        command.process_group(group);
        let fd = self.terminal.as_raw_fd();
        // SAFETY: only async-signal-safe libc calls run between fork and exec.
        unsafe {
            command.pre_exec(move || {
                let target = if group == 0 {
                    nix::libc::getpgrp()
                } else {
                    group
                };
                if nix::libc::tcsetpgrp(fd, target) == -1 {
                    return Err(io::Error::last_os_error());
                }
                for sig in [
                    nix::libc::SIGINT,
                    nix::libc::SIGQUIT,
                    nix::libc::SIGTTIN,
                    nix::libc::SIGTTOU,
                    nix::libc::SIGPIPE,
                ] {
                    nix::libc::signal(sig, nix::libc::SIG_DFL);
                }
                // No suspended jobs in v1: Ctrl+Z must not strand a child.
                nix::libc::signal(nix::libc::SIGTSTP, nix::libc::SIG_IGN);
                Ok(())
            });
        }
    }
}

pub struct ForegroundGuard<'a> {
    job: &'a JobControl,
    saved: Termios,
}

impl Drop for ForegroundGuard<'_> {
    fn drop(&mut self) {
        let _ = unistd::tcsetpgrp(&self.job.terminal, self.job.shell_group);
        let _ = termios::tcsetattr(&self.job.terminal, SetArg::TCSADRAIN, &self.saved);
    }
}
