#![cfg(target_os = "linux")]

use nix::{
    pty::{Winsize, openpty},
    sys::{
        signal::{Signal, killpg},
        termios::{self, LocalFlags},
    },
    unistd::{Pid, tcgetpgrp},
};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Terminal {
    child: Child,
    master: File,
    slave: File,
    bytes: Vec<u8>,
    original: termios::Termios,
}

impl Terminal {
    fn new() -> Self {
        Self::configured(|_| {})
    }

    fn configured(configure: impl FnOnce(&mut Command)) -> Self {
        let pty = openpty(
            Some(&Winsize {
                ws_row: 24,
                ws_col: 80,
                ws_xpixel: 0,
                ws_ypixel: 0,
            }),
            None,
        )
        .unwrap();
        let master = File::from(pty.master);
        let slave = File::from(pty.slave);
        let original = termios::tcgetattr(&slave).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_parsh"));
        command
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave.try_clone().unwrap()));
        configure(&mut command);
        // SAFETY: child-only, async-signal-safe session/controlling-tty setup.
        unsafe {
            command.pre_exec(|| {
                if nix::libc::setsid() == -1 || nix::libc::ioctl(0, nix::libc::TIOCSCTTY, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        // SAFETY: fcntl only updates this valid, owned PTY master's status flags.
        unsafe {
            assert_ne!(
                nix::libc::fcntl(
                    master.as_raw_fd(),
                    nix::libc::F_SETFL,
                    nix::libc::O_NONBLOCK
                ),
                -1
            );
        }
        let mut terminal = Self {
            child,
            master,
            slave,
            bytes: Vec::new(),
            original,
        };
        terminal.expect(b"\x1b[?2004h", 0);
        terminal
    }

    fn send(&mut self, bytes: &[u8]) -> usize {
        self.drain();
        let offset = self.bytes.len();
        self.master.write_all(bytes).unwrap();
        offset
    }

    fn paste(&mut self, source: &str) -> usize {
        self.send(format!("\x1b[200~{source}\x1b[201~").as_bytes())
    }

    fn drain(&mut self) {
        let mut buffer = [0; 8192];
        loop {
            match self.master.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => self.bytes.extend_from_slice(&buffer[..count]),
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock
                        || error.raw_os_error() == Some(nix::libc::EIO) =>
                {
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => panic!("PTY read: {error}"),
            }
        }
    }

    fn expect(&mut self, needle: &[u8], offset: usize) -> usize {
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            self.drain();
            let bytes = &self.bytes[offset..];
            if needle.starts_with(b"\x1b") {
                if let Some(index) = bytes.windows(needle.len()).position(|w| w == needle) {
                    return offset + index + needle.len();
                }
            } else {
                let visible = visible_bytes(bytes);
                let plain: Vec<_> = visible.iter().map(|&(byte, _)| byte).collect();
                if let Some(index) = plain.windows(needle.len()).position(|w| w == needle) {
                    return offset + visible[index + needle.len() - 1].1 + 1;
                }
            }
            assert!(
                Instant::now() < deadline,
                "missing {:?}\n{}",
                String::from_utf8_lossy(needle),
                String::from_utf8_lossy(&self.bytes)
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn ready_after(&mut self, offset: usize) {
        self.expect(b"\x1b[?2004h", offset);
    }

    fn exit(&mut self, code: i32) {
        self.send(b"\x03");
        // The interrupt is a raw-mode editor key, and then exit is a fresh form.
        let offset = self.bytes.len();
        self.expect(b"^C", offset);
        self.ready_after(offset);
        self.send(format!("exit {code})").as_bytes());
        self.wait_exit(code);
    }

    fn wait_exit(&mut self, code: i32) {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            self.drain();
            if let Some(status) = self.child.try_wait().unwrap() {
                assert_eq!(
                    status.code(),
                    Some(code),
                    "{}",
                    String::from_utf8_lossy(&self.bytes)
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "shell did not exit: {}",
                String::from_utf8_lossy(&self.bytes)
            );
            thread::sleep(Duration::from_millis(5));
        }
        let restored = termios::tcgetattr(&self.slave).unwrap();
        assert_eq!(restored.local_flags, self.original.local_flags);
        assert_eq!(restored.input_flags, self.original.input_flags);
        assert_eq!(restored.output_flags, self.original.output_flags);
    }
}

// Keep raw offsets so that output can be matched independently of CSI commands.
fn visible_bytes(bytes: &[u8]) -> Vec<(u8, usize)> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"\x1b[") {
            index += 2;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
        } else {
            result.push((bytes[index], index));
            index += 1;
        }
    }
    result
}

fn visible_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(
        &visible_bytes(bytes)
            .iter()
            .map(|&(byte, _)| byte)
            .collect::<Vec<_>>(),
    )
    .into_owned()
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            if let Ok(group) = tcgetpgrp(&self.master) {
                let _ = killpg(group, Signal::SIGKILL);
            }
            let _ = killpg(Pid::from_raw(self.child.id() as i32), Signal::SIGKILL);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[test]
fn enter_does_not_execute_and_outer_closer_does() {
    let mut terminal = Terminal::new();
    let offset = terminal.send(b"echo ENTER_TEST\r");
    thread::sleep(Duration::from_millis(100));
    terminal.drain();
    assert!(!visible_text(&terminal.bytes[offset..]).contains("\r\nENTER_TEST\r\n"));
    terminal.send(b")");
    let end = terminal.expect(b"\r\nENTER_TEST\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"echo \"x)\" (printf y))");
    let end = terminal.expect(b"\r\nx) y\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn full_script_paste_recovers_every_extra_closer() {
    let mut terminal = Terminal::new();
    let offset = terminal.paste("(echo PASTE_A))) (echo PASTE_B)");
    let end = terminal.expect(b"\r\nPASTE_B\r\n", offset);
    terminal.ready_after(end);
    let transcript = String::from_utf8_lossy(&terminal.bytes[offset..]);
    assert_eq!(transcript.matches("unexpected ')'").count(), 2);
    assert!(transcript.contains("paste:1:15:"), "{transcript}");
    assert!(transcript.contains("paste:1:16:"), "{transcript}");
    assert!(!transcript.contains("((echo"), "{transcript}");
    // A paste beginning with a command body consumes its implicit '(' only once.
    let offset = terminal.paste("echo BODY_A))) (echo BODY_B)");
    let end = terminal.expect(b"\r\nBODY_B\r\n", offset);
    terminal.ready_after(end);
    assert_eq!(
        String::from_utf8_lossy(&terminal.bytes[offset..])
            .matches("unexpected ')'")
            .count(),
        2
    );
    terminal.exit(0);
}

#[test]
fn incomplete_paste_tail_stays_editable() {
    let mut terminal = Terminal::new();
    let offset = terminal.paste("(echo FIRST)\n(echo SECOND");
    let end = terminal.expect(b"\r\nFIRST\r\n", offset);
    terminal.ready_after(end);
    terminal.send(b")");
    let end = terminal.expect(b"\r\nSECOND\r\n", end);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn runtime_failure_stops_execution_but_reports_remaining_closers() {
    let mut terminal = Terminal::new();
    let offset = terminal.paste("(false))) (echo BAD_AFTER_FAILURE)");
    let end = terminal.expect(b"unexpected ')'", offset);
    terminal.expect(b"unexpected ')'", end);
    terminal.ready_after(end);
    thread::sleep(Duration::from_millis(60));
    terminal.drain();
    assert!(!visible_text(&terminal.bytes[offset..]).contains("\r\nBAD_AFTER_FAILURE\r\n"));
    terminal.send(b"\x04");
    terminal.wait_exit(1);
}

#[test]
fn foreground_interrupt_restores_terminal_and_reaps_child() {
    let mut terminal = Terminal::new();
    let offset = terminal.send(b"sleep 30)");
    terminal.expect(b"\x1b[?2004l", offset);
    let deadline = Instant::now() + Duration::from_secs(3);
    let group = loop {
        let group = tcgetpgrp(&terminal.master).unwrap();
        if group.as_raw() != terminal.child.id() as i32 {
            break group;
        }
        assert!(Instant::now() < deadline, "no foreground handoff");
        thread::sleep(Duration::from_millis(5));
    };
    assert!(
        termios::tcgetattr(&terminal.slave)
            .unwrap()
            .local_flags
            .contains(LocalFlags::ICANON)
    );
    let offset = terminal.send(b"\x03");
    let end = terminal.expect(b"status 130", offset);
    terminal.ready_after(end);
    assert_eq!(
        tcgetpgrp(&terminal.master).unwrap().as_raw(),
        terminal.child.id() as i32
    );
    assert!(
        killpg(group, None).is_err(),
        "foreground child was not reaped"
    );
    let offset = terminal.send(b"echo ALIVE)");
    let end = terminal.expect(b"\r\nALIVE\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn queued_paste_never_becomes_child_stdin() {
    let mut terminal = Terminal::new();
    let offset = terminal
        .paste(r#"(sh -c "IFS= read -r line; printf 'GOT:%s\n' \"$line\"") (echo NEXT_COMMAND)"#);
    terminal.expect(b"\x1b[?2004l", offset);
    let deadline = Instant::now() + Duration::from_secs(3);
    while tcgetpgrp(&terminal.master).unwrap().as_raw() == terminal.child.id() as i32 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    terminal.send(b"live-input\n");
    terminal.expect(b"\r\nGOT:live-input\r\n", offset);
    let end = terminal.expect(b"\r\nNEXT_COMMAND\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn unicode_editing_and_history_require_explicit_submission() {
    let mut terminal = Terminal::new();
    let offset = terminal.paste("echo 甲e\u{301}👩‍💻乙");
    terminal.send(b"\x7f\x7f)");
    let end = terminal.expect("\r\n甲e\u{301}\r\n".as_bytes(), offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"\x1b[A");
    thread::sleep(Duration::from_millis(80));
    terminal.drain();
    assert!(!visible_text(&terminal.bytes[offset..]).contains("\r\n甲e\u{301}\r\n"));
    terminal.send(b")");
    let end = terminal.expect("\r\n甲e\u{301}\r\n".as_bytes(), offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn leading_extra_closers_are_not_empty_calls() {
    let mut terminal = Terminal::new();
    let offset = terminal.paste("))) (echo AFTER_CLOSERS)");
    let end = terminal.expect(b"\r\nAFTER_CLOSERS\r\n", offset);
    terminal.ready_after(end);
    let transcript = visible_text(&terminal.bytes[offset..]);
    assert_eq!(transcript.matches("unexpected ')'").count(), 3);
    for column in 1..=3 {
        assert!(transcript.contains(&format!("paste:1:{column}:")));
    }
    terminal.exit(0);
}

#[test]
fn foreground_pipeline_reads_terminal_and_restores_child_terminal_changes() {
    let mut terminal = Terminal::new();
    let offset = terminal.send(b"| (cat) (tr a-z A-Z))");
    terminal.expect(b"\x1b[?2004l", offset);
    let deadline = Instant::now() + Duration::from_secs(3);
    while tcgetpgrp(&terminal.master).unwrap().as_raw() == terminal.child.id() as i32 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    terminal.send(b"pipeline-input\n\x04");
    let end = terminal.expect(b"\r\nPIPELINE-INPUT\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"stty -echo)");
    terminal.expect(b"\x1b[?2004l", offset);
    terminal.ready_after(offset);
    terminal.exit(0);
}

#[test]
fn tab_completes_path_programs_files_and_variables_through_real_terminal_keys() {
    use std::{fs, os::unix::fs::PermissionsExt};

    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("tab-program");
    fs::write(&program, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(directory.path().join("fruits.txt"), "data").unwrap();
    fs::write(directory.path().join("你好 file.txt"), "data").unwrap();
    let mut terminal = Terminal::configured(|command| {
        command
            .current_dir(directory.path())
            .env("PATH", directory.path());
    });
    let offset = terminal.send(b"tab-pro\tTAB_COMMAND)");
    let end = terminal.expect(b"\r\nTAB_COMMAND\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"str fru\t)");
    let end = terminal.expect(b"\r\n\"fruits.txt\"\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.paste("(let TAB_VALUE retained)");
    terminal.ready_after(offset);
    let offset = terminal.send(b"str $TAB_V\t)");
    let end = terminal.expect(b"\r\n\"retained\"\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send("str 你\t)".as_bytes());
    let end = terminal.expect("\r\n\"你好 file.txt\"\r\n".as_bytes(), offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn double_tab_prints_candidates_and_keeps_editing_the_same_expression() {
    let mut terminal = Terminal::new();
    let offset =
        terminal.paste("(defn tab-alpha () (str TAB_ALPHA)) (defn tab-alpine () (str TAB_ALPINE))");
    terminal.ready_after(offset);
    let offset = terminal.send(b"tab-a\t");
    terminal.expect(b"(tab-alp", offset);
    let offset = terminal.send(b"\t");
    let end = terminal.expect(b"\r\ntab-alpha\r\ntab-alpine\r\n", offset);
    terminal.expect(b"(tab-alp", end);
    let offset = terminal.send(b"h\t)");
    let end = terminal.expect(b"\r\n\"TAB_ALPHA\"\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}
