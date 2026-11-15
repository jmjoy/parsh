use super::*;

fn check(source: &str, expected: &[&str]) {
    let result = evaluate(&mut Repl::new(), source);
    assert!(
        !result
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(_))),
        "{source}: {result:?}"
    );
    assert_eq!(values(&result), expected, "{source}");
}

fn fails(source: &str, expected: &str) {
    let result = evaluate(&mut Repl::new(), source);
    assert!(
        result
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(message) if message.contains(expected))),
        "{source}: {result:?}"
    );
}

#[test]
fn prelude_macros_branch_lazily_and_preserve_typed_conditions() {
    check(
        r#"
        (let calls #0)
        (list (when #true (set calls (+ $calls #1)) [a])
              (unless #false (set calls (+ $calls #1)) [b])
              (when #false $missing) (unless #true $missing) $calls)
        (cond ((= #1 #2) $missing) ((= #2 #2) (str selected)) (else $missing))
        (list (cond) (cond (#false $missing)) (when #true))
        (cond (else #7))
    "#,
        &[
            r#"(("a") ("b") #nil #nil #2)"#,
            r#""selected""#,
            "(#nil #nil #nil)",
            "#7",
        ],
    );
    fails("(cond (else #1) (#true #2))", "else clause must be last");
    fails("(cond (#true))", "condition and a body");
    fails("(when text #1)", "condition must be a boolean");
    fails("(function when)", "is not a function");
}

#[test]
fn threading_macros_insert_values_once_and_expand_without_execution() {
    check(
        r#"
        (let calls #0)
        (-> (do (set calls (+ $calls #1)) {n #2}) (map/get n) (+ #3))
        (->> [#1 #2 #3 #4] (list/filter even?) (list/map (partial + #10)))
        (list (-> #7) (->> #8) $calls)
        (macroexpand-1 (quote (-> #1 (+ #2) (* #3))))
        (macroexpand-1 (quote (when #false $missing)))
    "#,
        &[
            "#5",
            "(#12 #14)",
            "(#7 #8 #1)",
            "(* (+ #1 #2) #3)",
            "(if #false (do $missing))",
        ],
    );
    fails("(-> #1 ())", "steps cannot be empty");
    fails("(->>)", "arguments");
}

#[test]
fn form_operations_distinguish_words_variables_and_literal_data() {
    check(
        r#"
        (let code (quote (+ #1 $n)))
        (list (form/kind $code) (list/map (function form/kind) (form/children $code)))
        (list (form/value (quote hello)) (form/value (quote "hello"))
              (form/value (quote $n)) (form/value (quote #3)) (form/value (quote #nil)))
        (list (form/word plus) (form/variable n) (form/variable (quote n))
              (form/word (quote $n)) (form/source (quote (+ #1 #2))))
        (let data {text '$n' nested [#1 #true #nil]})
        (eq $data (eval (form/from-value $data)))
        (eval (form/list [(quote +) (form/from-value #2) (form/from-value #3)]))
    "#,
        &[
            r#"("list" ("word" "int" "variable"))"#,
            r#"("hello" "hello" "n" #3 #nil)"#,
            r#"(plus $n $n n "(+ #1 #2)")"#,
            "#true",
            "#5",
        ],
    );
    fails("(form/word \"two words\")", "one bare word");
    fails("(form/variable invalid-name)", "invalid variable name");
    fails("(form/children (quote #1))", "list or interpolated form");
    fails("(form/value (quote (a)))", "atomic form");
    fails("(form/list [#1])", "expected a form");
    fails("(form/from-value (fn () #1))", "expects literal data");
}

#[test]
fn read_and_eval_keep_parsing_explicit_and_use_the_caller_scope() {
    check(
        r#"
        (let n #10)
        (eval (read '(+ $n #2)'))
        (eval (quote (let declared #3)))
        (+ $declared #1)
        (defn local (n) (eval (read '(+ $n #1)')))
        (local #20)
        (list (eq (read "(; ignored) (+ #1 #2) (; tail)") (quote (+ #1 #2)))
              (eval (read "'literal'")))
        (let evaluator (function eval))
        ($evaluator (quote (set n #30)))
        (apply eval [(quote (set n #40))])
        (+ $n)
    "#,
        &["#12", "#4", "#21", r#"(#true "literal")"#, "#40"],
    );
    fails("(read \"\")", "exactly one form");
    fails("(read \"#1 #2\")", "exactly one form");
    fails("(read \"(\")", "<read>:1:1");
    fails("(eval \"(+ #1 #2)\")", "expected a form");
    fails("(eval (read '$missing'))", "in eval: undefined variable");
    fails("(load missing.parsh)", "requires a filesystem host");
}

#[test]
fn macroexpand_runs_one_macro_body_without_evaluating_the_result() {
    check(
        r#"
        (let expanded #0)
        (defmacro inner () (quote $missing))
        (defmacro outer () (set expanded (+ $expanded #1)) (quote (inner)))
        (list (macroexpand-1 (quote (outer))) $expanded)
        (macroexpand-1 (quote (+ #1 #2)))
        (macroexpand-1 (quote #1))
    "#,
        &["((inner) #1)", "(+ #1 #2)", "#1"],
    );
    fails(
        "(defmacro bad () #1) (macroexpand-1 (quote (bad)))",
        "expected a form",
    );
}

#[test]
fn gensym_has_unforgeable_identity_and_can_build_macro_bindings() {
    check(
        r#"
        (let first (gensym temp))
        (let second (gensym temp))
        (list (eq $first $first) (eq $first $second)
              (eq $first (form/word (form/value $first)))
              (eq $first (form/word (form/variable $first))))
        (let ordinary (form/word (form/value $first)))
        (eval (form/list [(quote let) $ordinary (quote #99)]))
        (eval (form/list [(quote let) $first (quote #7)]))
        (list (eval (form/variable $first)) (eval (form/variable $ordinary)))
        (defmacro twice (expression)
          (let temp (gensym temp))
          (let reference (form/variable $temp))
          `(do (let ,$temp ,$expression) (+ ,$reference ,$reference)))
        (let calls #0)
        (let temp #100)
        (list (twice (do (set calls (+ $calls #1)) #3)) $calls $temp)
    "#,
        &["(#true #false #false #true)", "(#7 #99)", "(#6 #1 #100)"],
    );
    fails(
        "(gensym invalid-name)",
        "prefix must be a valid variable name",
    );
    fails("(read (form/source (gensym)))", "invalid number");
}

#[test]
fn form_construction_has_a_depth_limit_and_read_errors_are_catchable() {
    fails(
        r#"(let node (quote a)) (for n (list/range #256) (set node (form/list [$node])))"#,
        "nesting exceeds 256",
    );
    check(
        r#"(try (read "(") (catch error (Error.kind $error)))"#,
        &[r#""read""#],
    );
}

#[test]
fn loading_uses_an_injected_filesystem_and_keeps_nested_relative_paths() {
    use crate::host::{CommandOutput, FileMode, HostStreams, OutputSink};
    use std::{collections::HashMap, ffi::OsString};
    struct Files(HashMap<String, Vec<u8>>);
    impl ExecutionHost for Files {
        fn reset(&mut self) -> Result<String, HostError> {
            Ok("/sandbox".into())
        }
        fn read_file(&mut self, _: &str, path: &str) -> Result<Vec<u8>, HostError> {
            self.0.get(path).cloned().ok_or_else(|| HostError {
                code: 2,
                message: format!("missing file: {path}"),
            })
        }
        fn run(
            &mut self,
            _: &[Vec<String>],
            _: &str,
            _: &HashMap<OsString, OsString>,
            _: HostStreams,
            _: OutputSink,
        ) -> Result<CommandOutput, HostError> {
            unreachable!()
        }
        fn open(&mut self, _: &str, _: &str, _: FileMode) -> Result<u32, HostError> {
            unreachable!()
        }
        fn close(&mut self, _: u32) {
            unreachable!()
        }
        fn chdir(&mut self, _: &str, _: &str) -> Result<String, HostError> {
            unreachable!()
        }
        fn paths(&mut self) -> Result<Vec<String>, HostError> {
            Ok(self.0.keys().cloned().collect())
        }
    }
    let mut repl = Repl::with_host(Files(HashMap::from([
        (
            "/sandbox/a.parsh".into(),
            b"(load sub/b.parsh) (defn loaded-fn () (+ $nested #1)) #5".to_vec(),
        ),
        ("/sandbox/sub/b.parsh".into(), b"(let nested #7)".to_vec()),
    ])))
    .unwrap();
    let result = evaluate(&mut repl, "(load a.parsh) (loaded-fn) (+ $nested)");
    assert!(
        !result
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(_))),
        "{result:?}"
    );
    assert_eq!(values(&result), ["#5", "#8", "#7"]);
}
