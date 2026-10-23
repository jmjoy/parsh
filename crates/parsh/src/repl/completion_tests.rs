use super::*;
use crate::host::{CommandOutput, FileMode, HostStreams, OutputChunk};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::OsString,
};

#[derive(Clone)]
struct TestHost {
    paths: Rc<RefCell<Vec<String>>>,
    runs: Rc<Cell<usize>>,
    queries_fail: Rc<Cell<bool>>,
}

impl Default for TestHost {
    fn default() -> Self {
        Self {
            paths: Rc::new(RefCell::new(
                [
                    "/",
                    "/home/",
                    "/home/playground/",
                    "/tmp/",
                    "/tmp/after",
                    "/home/playground/alpha",
                    "/home/playground/alpine",
                    "/home/playground/.hidden",
                    "/home/playground/sub/",
                    "/home/playground/sub/child.rs",
                    "/home/playground/sub-file",
                    "/home/playground/folder name/",
                    "/home/playground/folder name/你好.txt",
                    "/home/playground/$literal",
                    "/home/playground/shared",
                    "/home/playground/你好甲",
                    "/home/playground/你好乙",
                    "/home/playground/👩‍💻file",
                    "/home/playground/odd (\"x\")\n\t\\name",
                ]
                .map(String::from)
                .into(),
            )),
            runs: Rc::new(Cell::new(0)),
            queries_fail: Rc::new(Cell::new(false)),
        }
    }
}

impl ExecutionHost for TestHost {
    fn reset(&mut self) -> Result<String, HostError> {
        Ok("/home/playground".into())
    }
    fn run(
        &mut self,
        stages: &[Vec<String>],
        _: &str,
        _: &HashMap<OsString, OsString>,
        _: HostStreams,
    ) -> Result<CommandOutput, HostError> {
        self.runs.set(self.runs.get() + 1);
        Ok(CommandOutput {
            code: 0,
            chunks: vec![OutputChunk {
                stream: Stream::Stdout,
                bytes: stages[0][1..].join(" ").into_bytes(),
            }],
        })
    }
    fn open(&mut self, _: &str, _: &str, _: FileMode) -> Result<u32, HostError> {
        panic!("completion must not open files")
    }
    fn close(&mut self, _: u32) {
        panic!("completion must not close files")
    }
    fn chdir(&mut self, _: &str, path: &str) -> Result<String, HostError> {
        Ok(path.into())
    }
    fn commands(&mut self) -> Result<Vec<String>, HostError> {
        if self.queries_fail.get() {
            return Err(HostError {
                code: 2,
                message: "command query failed".into(),
            });
        }
        Ok(["echo", "host-alpine", "host-alpha", "host-alpha", "exit"]
            .map(String::from)
            .into())
    }
    fn paths(&mut self) -> Result<Vec<String>, HostError> {
        if self.queries_fail.get() {
            return Err(HostError {
                code: 2,
                message: "path query failed".into(),
            });
        }
        Ok(self.paths.borrow().clone())
    }
}

fn type_text(repl: &mut Repl, text: &str) -> Vec<Event> {
    text.chars()
        .flat_map(|ch| repl.input(Key::Character(ch)))
        .collect()
}

fn listings(events: &[Event]) -> Vec<&Vec<String>> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Completions { lines } => Some(lines),
            _ => None,
        })
        .collect()
}

#[test]
fn single_command_completes_without_running_or_changing_status_and_history() {
    let host = TestHost::default();
    let mut repl = Repl::with_host(host.clone()).unwrap();
    repl.session.last_code = 17;
    type_text(&mut repl, "ec");
    let events = repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "echo ");
    assert!(
        events
            .iter()
            .all(|event| matches!(event, Event::Render { .. }))
    );
    assert_eq!(repl.session.last_code, 17);
    assert!(repl.session.history.entries.is_empty());
    assert_eq!(host.runs.get(), 0);
    type_text(&mut repl, "hello)");
    assert_eq!(host.runs.get(), 1);
    assert_eq!(repl.session.history.entries, ["(echo hello)"]);
}

#[test]
fn syntax_only_completion_uses_available_builtins_and_current_functions() {
    let mut repl = Repl::new();
    type_text(&mut repl, "st");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str ");
    repl.input(Key::ClearInput);
    repl.paste("(defn tab-function () (str result))");
    type_text(&mut repl, "tab-f");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "tab-function ");
    assert!(type_text(&mut repl, ")").contains(&Event::Value("\"result\"".into())));
    repl.reset();
    type_text(&mut repl, "tab-f");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "tab-f");
    repl.input(Key::ClearInput);
    type_text(&mut repl, "cd");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "cd");
}

#[test]
fn numeric_builtins_complete_without_an_execution_host() {
    for operator in ["+", "-", "*", "/", "=", "!=", "<=", ">="] {
        let mut repl = Repl::new();
        type_text(&mut repl, operator);
        repl.input(Key::Tab);
        assert_eq!(repl.session.input.body, format!("{operator} "));
    }
}

#[test]
fn comparison_prefixes_list_the_operators_and_input_redirection() {
    for (prefix, expected) in [("<", vec!["<", "<="]), (">", vec![">", ">="])] {
        let mut repl = Repl::new();
        type_text(&mut repl, prefix);
        repl.input(Key::Tab);
        let events = repl.input(Key::Tab);
        let expected: Vec<_> = expected.into_iter().map(str::to_owned).collect();
        assert_eq!(listings(&events), [&expected]);
        assert_eq!(repl.session.input.body, prefix);
    }
    let mut repl = Repl::with_host(TestHost::default()).unwrap();
    type_text(&mut repl, "<");
    repl.input(Key::Tab);
    let events = repl.input(Key::Tab);
    assert_eq!(
        listings(&events),
        [&vec!["<".to_owned(), "<=".to_owned(), "<|".to_owned()]]
    );
    type_text(&mut repl, "=");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "<= ");
}

#[test]
fn double_tab_lists_sorted_unique_candidates_and_then_restores_the_input() {
    let mut repl = Repl::with_host(TestHost::default()).unwrap();
    type_text(&mut repl, "ho");
    let events = repl.input(Key::Tab);
    assert!(listings(&events).is_empty());
    assert_eq!(repl.session.input.body, "host-alp");
    let events = repl.input(Key::Tab);
    assert_eq!(
        listings(&events),
        [&vec!["host-alpha".to_string(), "host-alpine".to_string()]]
    );
    assert_eq!(events.first(), Some(&Event::Commit));
    assert!(
        matches!(events.last(), Some(Event::Render { lines, column: 9, .. }) if lines == &["(host-alp"])
    );
    type_text(&mut repl, "h");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "host-alpha ");
}

#[test]
fn editing_and_pasting_reset_the_double_tab_state() {
    let mut repl = Repl::with_host(TestHost::default()).unwrap();
    type_text(&mut repl, "ho");
    repl.input(Key::Tab);
    repl.input(Key::Left);
    repl.input(Key::Right);
    assert!(listings(&repl.input(Key::Tab)).is_empty());
    assert!(!listings(&repl.input(Key::Tab)).is_empty());
    repl.paste("");
    assert!(listings(&repl.input(Key::Tab)).is_empty());
    assert!(!listings(&repl.input(Key::Tab)).is_empty());
    repl.input(Key::Interrupt);
    assert!(repl.session.completion.is_none());
    assert!(repl.session.input.body.is_empty());
}

#[test]
fn paths_respect_working_directory_hidden_files_and_directory_only_cd() {
    let mut repl = Repl::with_host(TestHost::default()).unwrap();
    type_text(&mut repl, "cd s");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "cd sub/");
    repl.input(Key::ClearInput);
    type_text(&mut repl, "str sub/c");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str sub/child.rs ");
    repl.input(Key::ClearInput);
    type_text(&mut repl, "str ../playground/sub/c");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str ../playground/sub/child.rs ");
    repl.input(Key::ClearInput);
    type_text(&mut repl, "str /home/playground/sub/c");
    repl.input(Key::Tab);
    assert_eq!(
        repl.session.input.body,
        "str /home/playground/sub/child.rs "
    );
    repl.input(Key::ClearInput);
    type_text(&mut repl, "str ");
    repl.input(Key::Tab);
    let events = repl.input(Key::Tab);
    assert!(
        !listings(&events)[0]
            .iter()
            .any(|line| line.contains(".hidden"))
    );
    assert!(listings(&events)[0].contains(&"\"$literal\"".into()));
    repl.input(Key::ClearInput);
    type_text(&mut repl, "str .");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str .hidden ");
    repl.input(Key::ClearInput);
    repl.paste("(cd /tmp)");
    type_text(&mut repl, "str af");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str after ");
}

#[test]
fn paths_with_quotes_whitespace_and_dollar_signs_remain_literal_values() {
    let host = TestHost::default();
    let mut repl = Repl::with_host(host.clone()).unwrap();
    type_text(&mut repl, "str odd");
    repl.input(Key::Tab);
    let events = type_text(&mut repl, ")");
    assert!(events.contains(&Event::Value(
        Value::Text("odd (\"x\")\n\t\\name".into()).display()
    )));
    type_text(&mut repl, "str \"$l");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str \"$literal\" ");
    assert!(type_text(&mut repl, ")").contains(&Event::Value("\"$literal\"".into())));
    assert_eq!(host.runs.get(), 0);
}

#[test]
fn quoted_directory_completion_can_continue_into_the_next_component() {
    let mut repl = Repl::with_host(TestHost::default()).unwrap();
    type_text(&mut repl, "str fol");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str \"folder name/");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str \"folder name/你好.txt\" ");
    assert!(type_text(&mut repl, ")").contains(&Event::Value("\"folder name/你好.txt\"".into())));
}

#[test]
fn replacement_handles_mid_word_and_mid_string_without_losing_following_arguments() {
    let mut repl = Repl::with_host(TestHost::default()).unwrap();
    repl.session.input = Input {
        body: "echo shrt tail".into(),
        cursor: "echo sh".len(),
        explicit: false,
    };
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "echo shared tail");
    assert_eq!(repl.session.input.cursor, "echo shared".len());
    repl.session.input = Input {
        body: "str \"folwrong\" tail".into(),
        cursor: "str \"fol".len(),
        explicit: false,
    };
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str \"folder name/\" tail");
    assert_eq!(repl.session.input.cursor, "str \"folder name/".len());
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str \"folder name/你好.txt\" tail");
    assert_eq!(
        repl.session.input.cursor,
        "str \"folder name/你好.txt\"".len()
    );
}

#[test]
fn unicode_common_prefixes_and_editing_keep_valid_cursor_boundaries() {
    let mut repl = Repl::with_host(TestHost::default()).unwrap();
    type_text(&mut repl, "str 你");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str 你好");
    assert_eq!(repl.session.input.cursor, "str 你好".len());
    repl.input(Key::Backspace);
    assert_eq!(repl.session.input.body, "str 你");
    repl.input(Key::Tab);
    let events = repl.input(Key::Tab);
    assert_eq!(listings(&events)[0], &["你好乙", "你好甲"]);
    repl.input(Key::ClearInput);
    type_text(&mut repl, "str 👩‍💻");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str 👩‍💻file ");
}

#[test]
fn variable_completion_sees_executed_bindings_and_exports_but_not_values() {
    let mut repl = Repl::with_host(TestHost::default()).unwrap();
    repl.paste("(let TAB_NAME retained) (export TAB_ENV secret) (export TAB_NAME retained)");
    type_text(&mut repl, "str $TAB_");
    repl.input(Key::Tab);
    let events = repl.input(Key::Tab);
    assert_eq!(listings(&events)[0], &["$TAB_ENV", "$TAB_NAME"]);
    assert!(!format!("{events:?}").contains("secret"));
    type_text(&mut repl, "N");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str $TAB_NAME ");
    assert!(type_text(&mut repl, ")").contains(&Event::Value("\"retained\"".into())));
    repl.reset();
    type_text(&mut repl, "str $TAB_");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str $TAB_");
}

#[test]
fn unknown_prefixes_and_comments_are_no_ops_and_literal_pasted_tabs_survive() {
    let mut repl = Repl::with_host(TestHost::default()).unwrap();
    for text in ["no-such-prefix", "(; (ec", "(let na", "(defn f (na"] {
        repl.input(Key::ClearInput);
        type_text(&mut repl, text);
        repl.input(Key::Tab);
        let events = repl.input(Key::Tab);
        assert_eq!(repl.session.input.body, text);
        assert!(listings(&events).is_empty());
    }
    repl.input(Key::ClearInput);
    repl.paste("str\tunfinished");
    assert_eq!(repl.session.input.body, "str\tunfinished");
}

#[test]
fn host_lookup_failures_are_quiet_and_do_not_fall_back_to_native_capabilities() {
    let host = TestHost::default();
    host.queries_fail.set(true);
    let mut repl = Repl::with_host(host).unwrap();
    for text in ["ec", "ca", "str /home/jmjoy", "str $PATH"] {
        repl.input(Key::ClearInput);
        type_text(&mut repl, text);
        assert!(
            repl.input(Key::Tab)
                .iter()
                .all(|event| matches!(event, Event::Render { .. }))
        );
        assert_eq!(repl.session.input.body, text);
    }
    repl.input(Key::ClearInput);
    type_text(&mut repl, "st");
    repl.input(Key::Tab);
    assert_eq!(repl.session.input.body, "str ");
}

#[test]
fn narrow_candidate_lists_wrap_safely_and_restore_the_current_input() {
    let mut repl = Repl::with_host(TestHost::default()).unwrap();
    repl.resize(6, 5);
    type_text(&mut repl, "ho");
    repl.input(Key::Tab);
    let events = repl.input(Key::Tab);
    assert_eq!(listings(&events)[0], &["host-a", "lpha", "host-a", "lpine"]);
    assert!(
        matches!(events.last(), Some(Event::Render { lines, row: 1, column: 3, .. }) if lines == &["(host-", "alp"])
    );
}
