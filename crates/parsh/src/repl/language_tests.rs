use super::*;

fn evaluated(source: &str) -> Vec<String> {
    let events = execute(&mut Repl::new(), source);
    let errors: Vec<_> = events
        .iter()
        .filter_map(|event| {
            if let Event::Diagnostic(message) = event {
                Some(message)
            } else {
                None
            }
        })
        .collect();
    assert!(errors.is_empty(), "{source}: {errors:?}");
    events
        .into_iter()
        .filter_map(|event| {
            if let Event::Value(value) = event {
                Some(value)
            } else {
                None
            }
        })
        .collect()
}

fn fails(source: &str, message: &str) {
    let events = execute(&mut Repl::new(), source);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Diagnostic(text) if text.contains(message))),
        "{source}: {events:?}"
    );
}

fn values(events: &[Event]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Value(value) => Some(value.as_str()),
            _ => None,
        })
        .collect()
}

fn diagnostics(events: &[Event]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Diagnostic(message) => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn boolean_composition_short_circuits_and_requires_real_conditions() {
    assert_eq!(
        evaluated(
            r#"
        (let calls #0)
        (list (and) (or)
              (and (eq a b) (set calls (+ $calls #1)))
              (or (eq a a) (set calls (+ $calls #1)))
              (and (eq a a) (eq b b))
              (or (eq a b) (eq b b)) $calls)
    "#
        ),
        ["(#true #false #false #true #true #true #0)"]
    );
    fails("(and true)", "a condition must be a boolean");
    fails("(or (+ #1))", "a condition must be a boolean");
    fails("(and (eq a a) $missing)", "undefined variable");
}

#[test]
fn list_updates_preserve_nested_values_and_all_existing_copies() {
    assert_eq!(
        evaluated(
            r#"
        (let original [a {nested [b]}])
        (let alias $original)
        (let appended (list/append $original [c d] #true))
        (list $original $alias $appended)
        (list (list/concat [a] [b c]) (list/concat) (list/append []))
        (list (list/len $appended) (list/first $original) (list/rest $original)
              (list/get $appended #-1) (list/get $appended #-2)
              (list/first [] fallback) (list/rest []) (list/get [] #1 #nil))
        (list (list/slice $appended #1 #-1) (list/slice $original #-99 #99)
              (list/slice $original #2 #1) (list/slice $original #-1))
    "#
        ),
        [
            r#"(("a" (map "nested" ("b"))) ("a" (map "nested" ("b"))) ("a" (map "nested" ("b")) ("c" "d") #true))"#,
            r#"(("a" "b" "c") () ())"#,
            r#"(#4 "a" ((map "nested" ("b"))) #true ("c" "d") "fallback" () #nil)"#,
            r#"(((map "nested" ("b")) ("c" "d")) ("a" (map "nested" ("b"))) () ((map "nested" ("b"))))"#,
        ]
    );
    for source in ["(list/first [])", "(list/get [a] #-2)", "(list/get [] #0)"] {
        fails(source, "index out of range");
    }
    fails("(list/concat [a] text)", "expected a list");
    fails("(list/slice [a] #0.0)", "expected an integer");
}

#[test]
fn explicit_defaults_are_eager_once_in_all_call_forms() {
    assert_eq!(
        evaluated(
            r#"
        (let trace (str))
        (defn mark (part value) (set trace (str $trace $part)) $value)
        (let m {present #nil})
        (list (map/get (mark m $m) (mark k present) (mark d fallback)) $trace)
        (set trace (str))
        (let getter (function map/get))
        (list ($getter (mark m $m) (mark k present) (mark d fallback)) $trace)
        (set trace (str))
        (list (apply $getter [(mark m $m) (mark k present) (mark d fallback)]) $trace)
        (set trace (str))
        (list (list/get [found] #0 (mark a fallback))
              ((function list/first) [found] (mark b fallback))
              (apply (function str/get) [abc #0 (mark c fallback)]) $trace)
    "#
        ),
        [
            r#"(#nil "mkd")"#,
            r#"(#nil "mkd")"#,
            r#"(#nil "mkd")"#,
            r#"("found" "found" "a" "abc")"#
        ]
    );
    for source in [
        "(map/get {key value} key $undefined)",
        "((function map/get) {key value} key $undefined)",
        "(apply (function map/get) [{key value} key $undefined])",
        "(list/get [a] #0 $undefined)",
        "(list/first [a] $undefined)",
        "(str/get abc #0 $undefined)",
    ] {
        fails(source, "undefined variable");
    }
    assert_eq!(
        evaluated(
            r#"
        (let m {key original})
        (map/get $m key (do (set m {key changed}) fallback))
        (map/get $m key)
    "#
        ),
        [r#""original""#, r#""changed""#]
    );
}

#[test]
fn text_operations_count_chars_and_preserve_literal_patterns() {
    assert_eq!(
        evaluated(
            r#"
        (let text "中é👩‍💻")
        (list (str/len $text) (str/get $text #0) (str/get $text #-1)
              (str/slice $text #1 #3) (str/slice $text #-3)
              (str/slice $text #-99 #99) (str/slice $text #4 #1))
        (list (str/split "a,,b," ",") (str/split "中a" "")
              (str/join [a 中 b] "::") (str/lines "a\r\nb\n")
              (str/trim "　 a \n") (str/replace "a.a" "." "$")
              (str/contains "a.a" ".") (str/get "" #0 #nil))
        (list (str/split "" ",") (str/join [] ",") (str/lines ""))
    "#
        ),
        [
            r#"(#6 "中" "💻" "e\u{301}" "👩\u{200d}💻" "中e\u{301}👩\u{200d}💻" "")"#,
            r#"(("a" "" "b" "") ("" "中" "a" "") "a::中::b" ("a" "b") "a" "a\$a" #true #nil)"#,
            r#"(("") "" ())"#,
        ]
    );
    fails("(str/get abc #3)", "index out of range");
    fails("(str/len #3)", "expected text");
    fails("(str/join [a #3] ,)", "a list of text");
}

#[test]
fn type_queries_distinguish_text_numbers_nil_and_empty_collections() {
    assert_eq!(
        evaluated(
            r#"
        (defstruct Point x)
        (list (typeof 3) (typeof #3) (typeof #3.0) (typeof #true)
              (typeof []) (typeof {}) (typeof (quote a)) (typeof (Point a))
              (typeof (fn () #nil)) (typeof #nil))
        (list (text? 3) (number? 3) (number? #3) (int? #3)
              (float? #3.0) (bool? #true) (list? []) (map? {})
              (form? (quote a)) (struct? (Point a)) (function? (function +))
              (nil? #nil) (nil? nil) (status? 0)
              (empty? "") (empty? []) (empty? {}) (empty? [a]))
    "#
        ),
        [
            r#"("text" "int" "float" "bool" "list" "map" "form" "struct" "function" "nil")"#,
            "(#true #false #true #true #true #true #true #true #true #true #true #true #false #false #true #true #true #false)",
        ]
    );
    fails("(empty? #nil)", "empty? expects");
}

#[test]
fn numeric_helpers_handle_signs_precision_rounding_and_integer_limits() {
    assert_eq!(
        evaluated(
            r#"
        (list (quot #-7 #3) (rem #-7 #3) (quot #7 #-3) (rem #7 #-3)
              (abs #-4) (abs #-4.0) (floor #-1.2) (ceil #-1.2)
              (round #2.5) (round #-2.5) (round #2.49))
        (list (min #9007199254740993 #9007199254740992.0)
              (max #9007199254740993 #9007199254740992.0)
              (typeof (min #1 #1.0)) (typeof (max #1.0 #1))
              (round #9223372036854775807) (floor #-9223372036854775808.0))
    "#
        ),
        [
            "(#-2 #-1 #-2 #1 #4 #4.0 #-2 #-1 #3 #-3 #2)",
            r#"(#9007199254740992.0 #9007199254740993 "int" "float" #9223372036854775807 #-9223372036854775808)"#
        ]
    );
    for source in ["(quot #1 #0)", "(rem #1 #0)"] {
        fails(source, "division by zero");
    }
    for source in [
        "(quot #-9223372036854775808 #-1)",
        "(rem #-9223372036854775808 #-1)",
        "(abs #-9223372036854775808)",
    ] {
        fails(source, "overflow");
    }
    for source in [
        "(round #9223372036854775808.0)",
        "(ceil #1e100)",
        "(floor #-1e100)",
    ] {
        fails(source, "out of range");
    }
    fails("(quot #3.0 #2)", "expected an integer");
    fails("(min)", "arguments");
}

#[test]
fn independent_closures_can_be_returned_passed_and_stored() {
    assert_eq!(
        evaluated(
            r#"
        (defn counter (start)
          (fn (&optional (step #1)) (set start (+ $start $step)) $start))
        (let first (counter #0))
        (let second (counter #10))
        (defn invoke (f x) ($f $x))
        (list ($first) ($first #2) (invoke $second #5) ($first)
              ((list/get [$second] #0)) ((map/get {fn $first} fn)))
        (let alias $first)
        (list (eq $first $alias) (eq $first $second)
              (eq (function +) (function +))
              (eq (fn () a) (fn () a)))
    "#
        ),
        ["(#1 #3 #15 #4 #16 #5)", "(#true #false #true #false)"]
    );
    fails("(str (fn () a))", "got a function");
}

#[test]
fn function_references_retain_definitions_and_can_recurse() {
    assert_eq!(
        evaluated(
            r#"
        (defn named () (str old))
        (let old (function named))
        (defn named () (str new))
        (list ($old) (named) (eq $old (function named)))
        (let factorial (fn (n) (if (<= $n #1) #1 (* $n ($factorial (- $n #1))))))
        ($factorial #5)
        (let command str)
        ($command preserved)
    "#
        ),
        [r#"("old" "new" #false)"#, "#120", r#""preserved""#]
    );
    for name in [
        "if",
        "and",
        "let",
        "return",
        "fn",
        "quote",
        "exit",
        "no-such-command",
    ] {
        fails(&format!("(function {name})"), "is not a function");
    }
    fails(
        "(defmacro m () (quote a)) (function m)",
        "is not a function",
    );
}

#[test]
fn direct_referenced_and_applied_builtins_share_typed_operations() {
    for (name, arguments, expected) in [
        ("+", "#1 #2 #3", "#6"),
        ("/", "#5 #2", "#2.5"),
        ("<", "#9007199254740992.0 #9007199254740993", "#true"),
        ("not", "(eq a b)", "#true"),
        ("eq", "#3 3", "#false"),
        ("str", "a #1", r#""a1""#),
        ("list", "a #1", r#"("a" #1)"#),
        ("map", "key [a b]", r#"(map "key" ("a" "b"))"#),
        ("map/get", "{a b} a", r#""b""#),
        ("map/set", "{a b} c [d]", r#"(map "a" "b" "c" ("d"))"#),
        ("map/del", "{a b c d} a", r#"(map "c" "d")"#),
        ("map/has", "{a b} a", "#true"),
        ("map/keys", "{a b c d}", r#"("a" "c")"#),
        ("map/vals", "{a b c d}", r#"("b" "d")"#),
        ("map/len", "{a b}", "#1"),
        ("list/len", "[a b]", "#2"),
        ("list/first", "[a b]", r#""a""#),
        ("list/rest", "[a b]", r#"("b")"#),
        ("list/get", "[a b] #-1", r#""b""#),
        ("list/append", "[a] [b c]", r#"("a" ("b" "c"))"#),
        ("list/concat", "[a] [b c]", r#"("a" "b" "c")"#),
        ("list/concat", "", "()"),
        ("list/slice", "[a b c] #1 #-1", r#"("b")"#),
        ("str/len", "中文", "#2"),
        ("str/get", "中文 #-1", r#""文""#),
        ("str/slice", "中abc #1 #-1", r#""ab""#),
        ("str/split", "a,b ,", r#"("a" "b")"#),
        ("str/join", "[a b] ,", r#""a,b""#),
        ("str/lines", r#""a\r\nb\n""#, r#"("a" "b")"#),
        ("str/trim", r#""　 a \n""#, r#""a""#),
        ("str/replace", "a.a . '$'", r#""a\$a""#),
        ("str/contains", "a.a .", "#true"),
        ("quot", "#-7 #3", "#-2"),
        ("round", "#2.5", "#3"),
        ("typeof", "#2", r#""int""#),
        ("nil?", "#nil", "#true"),
    ] {
        let source = format!(
            "({name} {arguments}) ((function {name}) {arguments}) (apply (function {name}) [{arguments}]) (let callee {name}) ($callee {arguments})"
        );
        assert_eq!(
            evaluated(&source),
            [expected, expected, expected, expected],
            "{source}"
        );
    }
    assert_eq!(evaluated("(apply (function +) #1 #2 [#3 #4])"), ["#10"]);
    assert_eq!(
        evaluated("(let name value) (apply (function get) [name])"),
        [r#""value""#]
    );
    assert_eq!(evaluated("(apply + [#1])"), ["#1"]);
    fails("(apply (function +) 1)", "a final list");
}

#[test]
fn module_members_are_reserved_and_legacy_names_can_be_redefined() {
    for (old, name) in [
        ("slen", "str/len"),
        ("sget", "str/get"),
        ("sslice", "str/slice"),
        ("ssplit", "str/split"),
        ("sjoin", "str/join"),
        ("slines", "str/lines"),
        ("strim", "str/trim"),
        ("sreplace", "str/replace"),
        ("scontains", "str/contains"),
        ("llen", "list/len"),
        ("lfirst", "list/first"),
        ("lrest", "list/rest"),
        ("lget", "list/get"),
        ("lappend", "list/append"),
        ("lconcat", "list/concat"),
        ("lslice", "list/slice"),
        ("mget", "map/get"),
        ("mset", "map/set"),
        ("mdel", "map/del"),
        ("mhas", "map/has"),
        ("mkeys", "map/keys"),
        ("mvals", "map/vals"),
        ("mlen", "map/len"),
    ] {
        assert_eq!(
            evaluated(&format!("(function {name}) (str (type -t {name}))")),
            [format!("<function {name}>"), r#""builtin""#.into()],
        );
        fails(
            &format!("(defn {name} () #nil)"),
            "function name is reserved",
        );
        fails(
            &format!("(defmacro {name} () #nil)"),
            "macro name is reserved",
        );
        fails(
            &format!("(defstruct {name} field)"),
            "struct name is reserved",
        );
        fails(&format!("(function {old})"), "is not a function");
        assert_eq!(
            evaluated(&format!(
                "(if (type -t {old}) #true #false) (defn {old} () #7) ({old})"
            )),
            ["#false", "#7"],
        );
    }
    assert_eq!(
        evaluated("(defn str/custom () (str custom)) (str/custom)"),
        [r#""custom""#],
    );
}

#[test]
fn optional_defaults_are_per_call_and_rest_keeps_each_argument_whole() {
    assert_eq!(
        evaluated(
            r#"
        (let calls #0)
        (defn collect (x &optional y (z (do (set calls (+ $calls #1)) $x)) &rest tail)
          (list $x $y $z $tail))
        (collect first)
        (collect first second)
        (collect first second third [four five] #nil)
        (list $calls (apply (function collect) [a b c d e]))
        ((fn (&rest xs) $xs) [a b] c)
        (let order (str))
        (defn mark (part) (set order (str $order $part)) $part)
        (defn defaults (x &optional (y (mark d)) (z $y)) (list $x $y $z))
        (list (defaults (mark a)) $order)
    "#
        ),
        [
            r#"("first" #nil "first" ())"#,
            r#"("first" "second" "first" ())"#,
            r#"("first" "second" "third" (("four" "five") #nil))"#,
            r#"(#2 ("a" "b" "c" ("d" "e")))"#,
            r#"(("a" "b") "c")"#,
            r#"(("a" "d" "d") "ad")"#,
        ]
    );
    fails("(defn f (x &optional y) $x) (f)", "expected 1..2 arguments");
    fails(
        "(defn f (x &optional y) $x) (f 1 2 3)",
        "expected 1..2 arguments",
    );
    for parameters in [
        "(x &rest)",
        "(&rest x y)",
        "(&optional x &optional y)",
        "(x &rest x)",
        "(&optional (x))",
        "(&optional (x 1 2))",
        "(&rest (x 1))",
    ] {
        let events = execute(&mut Repl::new(), &format!("(fn {parameters} #nil)"));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Diagnostic(_))),
            "{parameters}"
        );
    }
}

#[test]
fn macros_bind_optional_and_rest_forms_without_evaluating_arguments() {
    assert_eq!(
        evaluated(
            r#"
        (defmacro block (&rest body) `(do ,@$body))
        (block (+ #1) (+ #2))
        (defmacro choose (condition &optional (yes (quote (+ #7))) &rest no)
          `(if ,$condition ,$yes (do ,@$no)))
        (choose (eq a a))
        (choose (eq a b) $undefined (+ #2) (+ #3))
        (defmacro same (x &optional (y $x)) $y)
        (same (+ #9))
        (defmacro absent (&optional x)
          (if (nil? $x) (quote (str absent)) $x))
        (absent)
        (absent (str present))
    "#
        ),
        ["#2", "#7", "#3", "#9", r#""absent""#, r#""present""#]
    );
}

#[test]
fn return_unwinds_loops_and_arguments_but_stops_at_the_current_function() {
    assert_eq!(
        evaluated(
            r#"
        (defn escape () (while #true (for x [a b] (return [done $x]))) $undefined)
        (escape)
        (defn inner () (return inner) $undefined)
        (defn outer () (inner) (return outer))
        (outer)
        (defn take (x) $undefined)
        (defn caller () (take (return [from argument])) $undefined)
        (caller)
        ((fn () (return (do))))
        (defn default_return (&optional (x (return early))) $undefined)
        (default_return)
        (defn applied () (apply (fn (x) (return $x)) [returned]))
        (applied)
    "#
        ),
        [
            r#"("done" "a")"#,
            r#""outer""#,
            r#"("from" "argument")"#,
            r#""early""#,
            r#""returned""#
        ]
    );
    fails("(return $undefined)", "return used outside");
    fails("(return 1 2)", "arguments");
    fails(
        "(defn broken () (break)) (while #true (broken))",
        "outside a loop body",
    );
}

#[test]
fn macros_have_their_own_return_boundary_and_preserve_expanded_jumps() {
    assert_eq!(
        evaluated(
            r#"
        (defmacro block (&rest body) (return `(do ,@$body)) $undefined)
        (block (+ #1) (+ #2))
        (defmacro early (value) `(return ,$value))
        (defn caller () (block (early [from macro])) $undefined)
        (caller)
        (defmacro stop (value) `(break ,$value))
        (while #true (stop done))
        (let count #0)
        (defmacro again () (quote (continue)))
        (while (< $count #2) (set count (+ $count #1)) (again) $undefined)
        (list $count)
    "#
        ),
        ["#2", r#"("from" "macro")"#, r#""done""#, "(#2)"]
    );
    fails(
        "(defmacro bad () (return 1)) (bad)",
        "must return a quoted form",
    );
    fails(
        "(defmacro early () (quote (return))) (early)",
        "return used outside",
    );
}

#[test]
fn list_forms_have_scopes_and_bindings_return_nil() {
    assert_eq!(
        evaluated(
            r#"
        (let root outer)
        (list (let local inner) $local)
        (str (let text value) $text)
        (list (set root changed) $root)
        (do (let block scoped) $block)
        "#,
        ),
        [
            r#"(#nil "inner")"#,
            r#""value""#,
            r#"(#nil "changed")"#,
            r#""scoped""#,
        ],
    );

    let mut repl = Repl::new();
    let events = execute(&mut repl, "(do (let local value) $local)");
    assert!(diagnostics(&events).is_empty(), "{events:?}");
    assert_eq!(values(&events), [r#""value""#]);
    assert!(diagnostics(&execute(&mut repl, "(get local)"))[0].contains("undefined variable"));
}

#[test]
fn let_rejects_duplicate_bindings_before_evaluating_the_rhs() {
    let mut repl = Repl::new();
    let events = execute(
        &mut repl,
        r#"
        (let trace before)
        (let value original)
        (let value (set trace changed))
        "#,
    );
    assert_eq!(diagnostics(&events).len(), 1, "{events:?}");
    assert!(diagnostics(&events)[0].contains("already bound in this scope"));
    assert!(values(&events).is_empty(), "{events:?}");
    assert_eq!(
        values(&execute(&mut repl, "(list $trace $value)")),
        [r#"("before" "original")"#]
    );
}

#[test]
fn nested_list_scopes_allow_shadowing_without_leaking_bindings() {
    assert_eq!(
        evaluated(
            r#"
        (do
          (let name outer)
          (list (do (let name inner) $name) $name))
        "#,
        ),
        [r#"("inner" "outer")"#],
    );

    fails(
        "(do (if #true (let branch value)) (get branch))",
        "undefined variable",
    );
}

#[test]
fn set_rejects_undefined_names_before_evaluating_the_rhs() {
    let mut repl = Repl::new();
    let events = execute(
        &mut repl,
        r#"
        (let trace before)
        (set missing (set trace changed))
        "#,
    );
    assert_eq!(diagnostics(&events).len(), 1, "{events:?}");
    assert!(diagnostics(&events)[0].contains("cannot set undefined variable missing"));
    assert!(values(&events).is_empty(), "{events:?}");
    assert_eq!(values(&execute(&mut repl, "(get trace)")), [r#""before""#]);
}

#[test]
fn set_resolves_its_destination_before_evaluating_the_rhs() {
    assert_eq!(
        evaluated(
            r#"
        (let value outer)
        (list (set value (let value inner)) $value)
        "#,
        ),
        ["(#nil #nil)"],
    );
}

#[test]
fn callable_names_and_explicit_command_references_are_first_class() {
    assert_eq!(
        evaluated(
            r#"
        (list (command? (command echo)) (callable? (function +))
              (callable? (command echo)) (bytes? hello) (typeof (command echo)))
        (apply + #1 [#2 #3])
        (apply "str/join" [[a b] ","])
        (apply "map/get" [{name parsh} name])
        (let r (capture-call + #1 #2))
        (list (CallResult.value $r) (CallResult.termination $r)
              (bytes? (CallResult.stdout $r)) (stdout-text $r))
        "#
        ),
        [
            r#"(#true #true #true #false "command")"#,
            "#6",
            r#""a,b""#,
            r#""parsh""#,
            r#"(#3 #nil #true "")"#
        ]
    );
    fails("(apply if [#true a b])", "not an ordinary callable");
    fails("(apply \"if\" [#true a b])", "not an ordinary callable");
    fails("(command #1)", "nonempty text program name");
}

#[test]
fn special_result_is_read_only_and_pure_queries_do_not_replace_it() {
    assert_eq!(
        evaluated(
            r#"
        (nil? $?)
        (defn value () #42)
        (value)
        (let saved $?)
        (list (CallResult.value $?) (CallResult.value (get ?))
              (eq $? $saved) (nil? (exit-code $?)) (nil? (signal $?)))
        "#
        ),
        ["#true", "#42", "(#42 #42 #true #true #true)"]
    );
    for source in [
        "(let ? #1)",
        "(set ? #1)",
        "(export ? a)",
        "(fn (?) #nil)",
        "(for ? [] #nil)",
    ] {
        fails(source, "$? is read-only");
    }
    assert_eq!(evaluated(r#"(str '\$?' "\$?")"#), [r#""\\\$?\$?""#]);
}

#[test]
fn exception_values_and_finally_preserve_function_returns() {
    assert_eq!(
        evaluated(
            r#"
        (let trace "")
        (defn answer ()
          (try (throw {reason failure})
            (catch e (set trace (str $trace caught))
                     (return (Error.payload $e)))
            (finally (set trace (str $trace cleaned)))))
        (list (answer) $trace)
        (try (try (throw original) (finally (throw replacement)))
          (catch e (Error.payload $e)))
        "#
        ),
        [
            r#"((map "reason" "failure") "caughtcleaned")"#,
            r#""replacement""#
        ]
    );
    fails(
        "(try #1 (finally (return #2)))",
        "return is not allowed in finally",
    );
    fails(
        "(try #1 (finally (for x [a] (break))))",
        "break is not allowed in finally",
    );
    assert_eq!(
        evaluated("(defn helper () (return #2)) (try #1 (finally (helper)))"),
        ["#1"]
    );
}

#[test]
fn malformed_try_is_validated_before_any_body_side_effect() {
    let mut repl = Repl::new();
    execute(&mut repl, "(let count #0)");
    for source in [
        "(try (set count #1) (catch e #2) (catch e #3))",
        "(try (set count #1) (finally #2) (catch e #3))",
        "(try (set count #1) (finally #2) #3)",
        "(try (set count #1))",
    ] {
        let events = execute(&mut repl, source);
        assert!(!diagnostics(&events).is_empty(), "{source}");
        assert_eq!(values(&execute(&mut repl, "(+ $count)")), ["#0"]);
    }
}

#[test]
fn result_interpolation_and_background_capability_are_explicit() {
    assert_eq!(
        evaluated(
            r#"
        (defn value () #7) (value)
        (str "value=$(CallResult.value $?)")
        "#
        ),
        ["#7", r#""value=7""#]
    );
    fails(r#"(defn value () #1) (value) (str "$?")"#, "got a struct");
    fails(
        "(spawn echo hello)",
        "requires native background-task support",
    );
    fails(
        "(CallResult #nil #nil #nil #nil #nil [])",
        "constructed by the runtime",
    );
    fails("(defstruct Job id)", "struct name is reserved");
}

#[test]
fn higher_order_capture_calls_have_a_bounded_stack() {
    let source = format!("({} + #1)", "capture-call ".repeat(256));
    fails(&source, "capture nesting exceeds 32 levels");
}
