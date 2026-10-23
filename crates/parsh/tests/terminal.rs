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

    fn execute(&mut self, source: &str) -> usize {
        let offset = self.paste(source);
        self.send(b"\r");
        offset
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
        self.send(format!("exit {code}\r").as_bytes());
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
fn prompt_uses_effective_uid_despite_the_user_environment() {
    let root = nix::unistd::geteuid().is_root();
    let prompt = if root { "# ()" } else { "$ ()" };
    let mut terminal = Terminal::configured(|command| {
        command.env("USER", if root { "ordinary-user" } else { "root" });
    });
    terminal.expect(prompt.as_bytes(), 0);
    let offset = terminal.send(b"str PROMPT_TEST\r");
    let end = terminal.expect(b"\r\n\"PROMPT_TEST\"\r\n", offset);
    terminal.ready_after(end);
    terminal.expect(prompt.as_bytes(), end);
    terminal.exit(0);
}

#[test]
fn enter_submits_and_typed_closers_remain_edits() {
    let mut terminal = Terminal::new();
    let offset = terminal.send(b"echo ENTER_TEST)");
    thread::sleep(Duration::from_millis(80));
    terminal.drain();
    assert!(!visible_text(&terminal.bytes[offset..]).contains("\r\nENTER_TEST\r\n"));
    terminal.send(b"\r");
    let end = terminal.expect(b"\r\nENTER_TEST\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"echo \"x)\" (printf y)\r");
    let end = terminal.expect(b"\r\nx) y\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn middle_enter_submits_the_whole_input() {
    let mut terminal = Terminal::new();
    let offset = terminal.send(b"echo FIRSTSECOND\x1b[D\x1b[D\x1b[D\r");
    let end = terminal.expect(b"\r\nFIRSTSECOND\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn paste_waits_for_enter_and_syntax_errors_prevent_the_entire_batch() {
    let mut terminal = Terminal::new();
    let offset = terminal.paste("(echo PASTE_A) (echo PASTE_B)");
    thread::sleep(Duration::from_millis(80));
    terminal.drain();
    assert!(!visible_text(&terminal.bytes[offset..]).contains("\r\nPASTE_A\r\n"));
    terminal.send(b"\r");
    let end = terminal.expect(b"\r\nPASTE_B\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.execute("(echo NEVER_EXECUTED))) (echo ALSO_SKIPPED)");
    terminal.expect(b"unexpected ')'", offset);
    terminal.ready_after(offset);
    terminal.drain();
    assert!(!visible_text(&terminal.bytes[offset..]).contains("\r\nNEVER_EXECUTED\r\n"));
    terminal.send(b"\x1f"); // The complete paste is still one undoable edit.
    let offset = terminal.send(b"echo RECOVERED\r");
    let end = terminal.expect(b"\r\nRECOVERED\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn incomplete_input_and_control_j_allow_multiline_editing() {
    let mut terminal = Terminal::new();
    let offset = terminal.paste("echo (str FIRST");
    terminal.send(b"\r");
    thread::sleep(Duration::from_millis(60));
    terminal.drain();
    assert!(!visible_text(&terminal.bytes[offset..]).contains("\r\nFIRST\r\n"));
    terminal.send(b"SECOND)\r");
    let end = terminal.expect(b"\r\nFIRSTSECOND\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"echo FIRST\nSECOND\r");
    let end = terminal.expect(b"\r\nFIRST SECOND\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn triple_quoted_terminal_input_waits_for_complete_delimiters() {
    let mut terminal = Terminal::new();
    for delimiter in ["\"\"\"", "'''"] {
        let offset = terminal.paste(&format!("echo {delimiter}FIRST"));
        terminal.send(b"\r");
        terminal.send(format!("SECOND{}\r", &delimiter[..2]).as_bytes());
        terminal.send(format!("THIRD{delimiter}\r").as_bytes());
        let expected = format!("\r\nFIRST\r\nSECOND{}\r\nTHIRD\r\n", &delimiter[..2]);
        let end = terminal.expect(expected.as_bytes(), offset);
        terminal.ready_after(end);
    }
    terminal.exit(0);
}

#[test]
fn runtime_failure_stops_the_batch_and_preserves_exit_status() {
    let mut terminal = Terminal::new();
    let offset = terminal.execute("(false) (echo BAD_AFTER_FAILURE)");
    terminal.expect(b"status 1", offset);
    terminal.ready_after(offset);
    terminal.drain();
    assert!(!visible_text(&terminal.bytes[offset..]).contains("\r\nBAD_AFTER_FAILURE\r\n"));
    terminal.send(b"\x04");
    terminal.wait_exit(1);
}

#[test]
fn foreground_interrupt_restores_terminal_and_reaps_child() {
    for source in ["sleep 30", "while (eq a a) (sleep 30)"] {
        let mut terminal = Terminal::new();
        let offset = terminal.send(format!("{source}\r").as_bytes());
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
        let offset = terminal.send(b"echo ALIVE\r");
        let end = terminal.expect(b"\r\nALIVE\r\n", offset);
        terminal.ready_after(end);
        terminal.exit(0);
    }
}

#[test]
fn interrupting_a_builtin_while_loop_restores_the_prompt() {
    let mut terminal = Terminal::new();
    let offset = terminal.execute("(while (eq a a) (continue)) (echo SKIPPED)");
    terminal.expect(b"\x1b[?2004l", offset);
    thread::sleep(Duration::from_millis(30));
    terminal.send(b"\x03");
    let end = terminal.expect(b"interrupted", offset);
    terminal.ready_after(end);
    terminal.drain();
    assert!(!visible_text(&terminal.bytes[offset..]).contains("\r\nSKIPPED\r\n"));
    assert_eq!(
        tcgetpgrp(&terminal.master).unwrap().as_raw(),
        terminal.child.id() as i32
    );
    let offset = terminal.execute("(break)");
    terminal.expect(b"outside a loop body", offset);
    terminal.ready_after(offset);
    let offset = terminal.execute("(echo ALIVE)");
    let end = terminal.expect(b"\r\nALIVE\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn queued_paste_never_becomes_child_stdin() {
    let mut terminal = Terminal::new();
    let offset = terminal.execute(
        r#"(sh -c "IFS= read -r line; printf 'GOT:%s\n' \"\$line\"") (echo NEXT_COMMAND)"#,
    );
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
    terminal.send(b"\x7f\x7f\r");
    let end = terminal.expect("\r\n甲e\u{301}\r\n".as_bytes(), offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"\x1b[A");
    thread::sleep(Duration::from_millis(80));
    terminal.drain();
    let recalled = visible_text(&terminal.bytes[offset..]);
    assert!(recalled.contains("(echo 甲e\u{301}"));
    assert!(recalled.contains("(echo 甲e\u{301})"));
    assert!(!recalled.contains("\r\n甲e\u{301}\r\n"));
    terminal.send(b"\r");
    let end = terminal.expect("\r\n甲e\u{301}\r\n".as_bytes(), offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"\x1b[A");
    terminal.expect("(echo 甲e\u{301}".as_bytes(), offset);
    terminal.send(" 追加\r".as_bytes());
    let end = terminal.expect("\r\n甲e\u{301} 追加\r\n".as_bytes(), offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn foreground_pipeline_reads_terminal_and_restores_child_terminal_changes() {
    let mut terminal = Terminal::new();
    let offset = terminal.send(b"| (cat) (tr a-z A-Z)\r");
    terminal.expect(b"\x1b[?2004l", offset);
    let deadline = Instant::now() + Duration::from_secs(3);
    while tcgetpgrp(&terminal.master).unwrap().as_raw() == terminal.child.id() as i32 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    terminal.send(b"pipeline-input\n\x04");
    let end = terminal.expect(b"\r\nPIPELINE-INPUT\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"stty -echo\r");
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
    let offset = terminal.send(b"tab-pro\tTAB_COMMAND\r");
    let end = terminal.expect(b"\r\nTAB_COMMAND\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"str fru\t\r");
    let end = terminal.expect(b"\r\n\"fruits.txt\"\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.execute("(let TAB_VALUE retained)");
    terminal.ready_after(offset);
    let offset = terminal.send(b"str $TAB_V\t\r");
    let end = terminal.expect(b"\r\n\"retained\"\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send("str 你\t\r".as_bytes());
    let end = terminal.expect("\r\n\"你好 file.txt\"\r\n".as_bytes(), offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn double_tab_prints_candidates_and_keeps_editing_the_same_expression() {
    let mut terminal = Terminal::new();
    let offset = terminal
        .execute("(defn tab-alpha () (str TAB_ALPHA)) (defn tab-alpine () (str TAB_ALPINE))");
    terminal.ready_after(offset);
    let offset = terminal.send(b"tab-a\t");
    terminal.expect(b"(tab-alp", offset);
    let offset = terminal.send(b"\t");
    let end = terminal.expect(b"\r\ntab-alpha\r\ntab-alpine\r\n", offset);
    terminal.expect(b"(tab-alp", end);
    let offset = terminal.send(b"h\t\r");
    let end = terminal.expect(b"\r\n\"TAB_ALPHA\"\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn emacs_character_word_transpose_and_undo_keys_use_real_control_bytes() {
    let mut terminal = Terminal::new();
    for (keys, expected) in [
        (b"echo ac\x02b\x06\r".as_slice(), "abc"),
        (b"echo XY\x08Z\r".as_slice(), "XZ"),
        (
            b"echo path/file.txt\x1bb\x1bdX\x05\r".as_slice(),
            "path/file.X",
        ),
        (b"echo one two\x17\x19\r".as_slice(), "one two"),
        (b"echo RAW\x02X\x1f\x05\r".as_slice(), "RAW"),
        (b"echo CHORD\x02X\x18\x15\x05\r".as_slice(), "CHORD"),
        ("echo 甲e\u{301}👩‍💻乙\x14\r".as_bytes(), "甲e\u{301}乙👩‍💻"),
    ] {
        let offset = terminal.send(keys);
        let end = terminal.expect(format!("\r\n{expected}\r\n").as_bytes(), offset);
        terminal.ready_after(end);
    }
    terminal.exit(0);
}

#[test]
fn emacs_alt_backspace_and_split_escape_prefix_share_the_kill_ring() {
    let mut terminal = Terminal::new();
    let offset = terminal.send(b"echo one two\x17\x1b\x7f\x19\r");
    let end = terminal.expect(b"\r\none two\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"echo one two\x17");
    terminal.expect(b"(echo one ", offset);
    terminal.send(b"\x1b");
    thread::sleep(Duration::from_millis(80));
    terminal.send(b"\x08\x19\r");
    let end = terminal.expect(b"\r\none two\r\n", offset);
    terminal.ready_after(end);
    // Move between kills so Alt+Y has a second entry to rotate to.
    let offset = terminal.send(b"echo first second\x17\x02\x17\x19\x1by\x05\r");
    let end = terminal.expect(b"\r\nsecond\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn emacs_search_accepts_without_submitting_and_abort_restores_the_cursor() {
    let mut terminal = Terminal::new();
    let offset = terminal.execute("(echo SEARCH_FIRST)(echo SEARCH_SECOND)");
    let end = terminal.expect(b"\r\nSEARCH_SECOND\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"echo saved\x02\x12SEARCH");
    terminal.expect(b"(reverse-i-search)`SEARCH':", offset);
    assert!(!visible_text(&terminal.bytes[offset..]).contains("\r\nSEARCH_SECOND\r\n"));
    let offset = terminal.send(b"\x12");
    terminal.expect(b"(echo SEARCH_FIRST", offset);
    let offset = terminal.send(b"\x13");
    terminal.expect(b"(echo SEARCH_SECOND", offset);
    let offset = terminal.send(b"\x07X\x05\r");
    let end = terminal.expect(b"\r\nsaveXd\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"\x12SEARCH_SECOND\r");
    terminal.expect(b"(echo SEARCH_SECOND", offset);
    terminal.drain();
    assert!(!visible_text(&terminal.bytes[offset..]).contains("\r\nSEARCH_SECOND\r\n"));
    terminal.send(b"\r");
    let end = terminal.expect(b"\r\nSEARCH_SECOND\r\n", offset);
    terminal.ready_after(end);
    terminal.exit(0);
}

#[test]
fn emacs_clear_screen_preserves_the_draft_and_search() {
    let mut terminal = Terminal::new();
    let offset = terminal.send(b"echo CLEAR_SCREEN\x0c");
    let end = terminal.expect(b"\x1b[2J", offset);
    terminal.expect(b"(echo CLEAR_SCREEN", end);
    terminal.send(b"\r");
    let end = terminal.expect(b"\r\nCLEAR_SCREEN\r\n", offset);
    terminal.ready_after(end);
    let offset = terminal.send(b"\x12CLEAR\x0c");
    let end = terminal.expect(b"\x1b[2J", offset);
    terminal.expect(b"(reverse-i-search)`CLEAR':", end);
    terminal.send(b"\x07");
    terminal.exit(0);
}
