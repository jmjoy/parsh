use super::*;

fn typed(repl: &mut Repl, text: &str) {
    for ch in text.chars() {
        let events = repl.input(Key::Character(ch));
        assert!(
            events.iter().all(|e| matches!(e, Event::Render { .. })),
            "{events:?}"
        );
    }
}

fn at(text: &str) -> Repl {
    let (left, right) = text.split_once('|').unwrap();
    let mut repl = Repl::new();
    repl.paste(&format!("{left}{right}"));
    repl.session.input.cursor = left.len();
    repl
}

fn state(repl: &Repl) -> String {
    let input = &repl.session.input;
    format!(
        "{}|{}",
        &input.body[..input.cursor],
        &input.body[input.cursor..]
    )
}

#[test]
fn typed_pairs_are_source_and_closers_only_move() {
    let mut repl = Repl::new();
    typed(&mut repl, "list ([{");
    assert_eq!(state(&repl), "list ([{|}])");
    typed(&mut repl, "value}])");
    assert_eq!(state(&repl), "list ([{value}])|");
    typed(&mut repl, ")");
    assert_eq!(state(&repl), "list ([{value}])|");
    assert!(!repl.session.input.explicit);
    repl.input(Key::ClearInput);
    typed(&mut repl, "(str hello");
    assert_eq!(state(&repl), "(str hello|)");
    assert!(repl.session.input.explicit);
    assert!(
        repl.input(Key::Enter)
            .contains(&Event::Value("\"hello\"".into()))
    );
}

#[test]
fn quotes_promote_only_after_three_consecutive_typed_quotes() {
    for quote in ['\'', '"'] {
        let mut repl = Repl::new();
        typed(&mut repl, "str ");
        typed(&mut repl, &quote.to_string());
        assert_eq!(state(&repl), format!("str {quote}|{quote}"));
        typed(&mut repl, &quote.to_string().repeat(2));
        assert_eq!(
            state(&repl),
            format!("str {0}|{0}", quote.to_string().repeat(3))
        );
        typed(&mut repl, "hello\nworld");
        typed(&mut repl, &quote.to_string().repeat(3));
        assert_eq!(
            state(&repl),
            format!("str {0}hello\nworld{0}|", quote.to_string().repeat(3))
        );
        assert!(
            repl.input(Key::Enter)
                .contains(&Event::Value("\"hello\\nworld\"".into()))
        );
        repl.input(Key::Undo);
        assert!(repl.session.input.body.is_empty());
    }
    let mut repl = at("str ''|");
    typed(&mut repl, "'");
    assert_eq!(state(&repl), "str '' '|'");
    let mut repl = Repl::new();
    typed(&mut repl, "str \"\"");
    repl.input(Key::Left);
    repl.input(Key::Right);
    typed(&mut repl, "\"");
    assert_eq!(state(&repl), "str \"\" \"|\"");
}

#[test]
fn strings_keep_literal_brackets_and_edit_embedded_calls() {
    let mut repl = Repl::new();
    typed(&mut repl, "str \"[({ $ ( raw) $(str hi)\"");
    assert_eq!(state(&repl), "str \"[({ $ ( raw) $(str hi)\"|");
    assert!(
        repl.input(Key::Enter)
            .contains(&Event::Value("\"[({ \\$ ( raw) hi\"".into()))
    );
    let mut repl = at("str \"a|b\"");
    typed(&mut repl, "\"");
    assert_eq!(state(&repl), "str \"a\\\"|b\"");
    typed(&mut repl, "\\n");
    assert_eq!(state(&repl), "str \"a\\\"\\n|b\"");
    repl.input(Key::Backspace);
    assert_eq!(state(&repl), "str \"a\\\"|b\"");
    repl.input(Key::Backspace);
    assert_eq!(state(&repl), "str \"a|b\"");
}

#[test]
fn deletion_crosses_nonempty_boundaries_and_removes_empty_pairs() {
    for (before, key, after) in [
        ("(|)", Key::Backspace, "|"),
        ("[|]", Key::Delete, "|"),
        ("str \"|\"", Key::Backspace, "str |"),
        ("str '''|'''", Key::Delete, "str |"),
        ("(a)|", Key::Backspace, "(a|)"),
        ("|(a)", Key::Delete, "(|a)"),
        ("(|a)", Key::Backspace, "|(a)"),
        ("(a|)", Key::Delete, "(a)|"),
        ("str \"a\\|nb\"", Key::Delete, "str \"a|b\""),
        ("str \"a\\n|b\"", Key::Backspace, "str \"a|b\""),
    ] {
        let mut repl = at(before);
        repl.input(key);
        assert_eq!(state(&repl), after, "{before} {key:?}");
    }
}

#[test]
fn transformations_preserve_source_and_cursor() {
    for (before, key, after) in [
        ("list |alpha beta", Key::WrapSexp, "list (|alpha) beta"),
        ("list (a| b) c", Key::SpliceSexp, "list a| b c"),
        ("(a|) b", Key::SlurpForward, "(a| b)"),
        ("(a| b)", Key::BarfForward, "(a|) b"),
        ("a (b|)", Key::SlurpBackward, "(a b|)"),
        ("(a b|)", Key::BarfBackward, "a (b|)"),
        ("[(a|)] b", Key::SlurpForward, "[(a|) b]"),
        ("(a|)b", Key::SlurpForward, "(a| b)"),
        ("a(b|)", Key::SlurpBackward, "(a b|)"),
        ("list a(|b)c", Key::SpliceSexp, "list a |b c"),
        ("list a(|)b", Key::SpliceSexp, "list a |b"),
        ("list |", Key::WrapSexp, "list (|)"),
        ("list {a| b} c", Key::SlurpForward, "list {a| b c}"),
        ("list [a| b]", Key::BarfForward, "list [a|] b"),
    ] {
        let mut repl = at(before);
        let events = repl.input(key);
        assert!(events.iter().all(|e| matches!(e, Event::Render { .. })));
        assert_eq!(state(&repl), after, "{before} {key:?}");
        repl.input(Key::Undo);
        assert_eq!(state(&repl), before, "undo {before} {key:?}");
    }
}

#[test]
fn implicit_root_and_interpolation_boundaries_are_protected() {
    for before in ["list a| b", "str \"$(str a| b)\""] {
        for key in [
            Key::SpliceSexp,
            Key::SlurpForward,
            Key::SlurpBackward,
            Key::BarfForward,
            Key::BarfBackward,
        ] {
            let mut repl = at(before);
            repl.input(key);
            assert_eq!(state(&repl), before, "{key:?}");
        }
    }
    let mut repl = at("str \"$(str (a|) b)\"");
    repl.input(Key::SlurpForward);
    assert_eq!(state(&repl), "str \"$(str (a| b))\"");
}

#[test]
fn wrapping_a_direct_literal_keeps_its_new_parentheses_explicit() {
    let mut repl = at("#tr|ue");
    assert!(!repl.session.input.explicit);
    repl.input(Key::WrapSexp);
    assert_eq!(state(&repl), "(#tr|ue)");
    assert_eq!(repl.session.input.source(), "(#true)");
    assert!(repl.session.input.explicit);
    repl.input(Key::Undo);
    assert_eq!(state(&repl), "#tr|ue");
    assert_eq!(repl.session.input.source(), "#true");
    assert!(!repl.session.input.explicit);
}

#[test]
fn navigation_killing_and_escape_prefixes_share_commands() {
    let mut repl = at("list |(str a) [b c]");
    repl.input(Key::Escape);
    repl.input(Key::Right);
    assert_eq!(state(&repl), "list (str a)| [b c]");
    repl.input(Key::SexpBackward);
    repl.input(Key::SexpDown);
    assert_eq!(state(&repl), "list (|str a) [b c]");
    repl.input(Key::SexpUp);
    repl.input(Key::KillSexp);
    assert_eq!(state(&repl), "list | [b c]");
    repl.input(Key::Yank);
    assert_eq!(state(&repl), "list (str a)| [b c]");
    repl.input(Key::Undo);
    assert_eq!(state(&repl), "list | [b c]");
    repl.input(Key::Undo);
    assert_eq!(state(&repl), "list |(str a) [b c]");
}

#[test]
fn existing_kills_and_transposition_do_not_split_pairs() {
    for (before, key, after) in [
        ("(str |a b)", Key::KillEnd, "(str |)"),
        ("(str a| b)", Key::KillStart, "(| b)"),
        ("str \"a|b\" c", Key::KillEnd, "str \"a|\" c"),
        ("list |(a b)", Key::KillWordForward, "list |(a b)"),
        ("(a| b)", Key::Transpose, "(a| b)"),
        ("str \"a\\n|b\"", Key::Transpose, "str \"a\\n|b\""),
    ] {
        let mut repl = at(before);
        repl.input(key);
        assert_eq!(state(&repl), after, "{before} {key:?}");
    }
}

#[test]
fn pasted_errors_can_be_repaired_without_rewriting_source() {
    let mut repl = at("[a}| ");
    repl.input(Key::Backspace);
    typed(&mut repl, "]");
    assert_eq!(state(&repl), "[a]| ");
    let mut repl = at("str 'unfinished|");
    typed(&mut repl, "'");
    assert_eq!(state(&repl), "str 'unfinished'|");
    let mut repl = at("(str [a|)");
    repl.input(Key::SlurpForward);
    assert_eq!(state(&repl), "(str [a|)");
    typed(&mut repl, "]");
    assert_eq!(state(&repl), "(str [a]|)");
}

fn readable(input: &Input) -> bool {
    let mut source = input.source();
    if input.implicit() {
        source.push(')');
    }
    let mut reader = Reader::new(&source);
    loop {
        match reader.read() {
            Read::Form(_) => {}
            Read::End => return true,
            _ => return false,
        }
    }
}

#[test]
fn structural_edits_at_every_grapheme_boundary_keep_complete_source_readable() {
    let keys = [
        Key::Backspace,
        Key::Delete,
        Key::KillStart,
        Key::KillEnd,
        Key::KillWordBack,
        Key::KillWordForward,
        Key::KillWhitespaceWord,
        Key::KillSexp,
        Key::Transpose,
        Key::WrapSexp,
        Key::SpliceSexp,
        Key::SlurpForward,
        Key::SlurpBackward,
        Key::BarfForward,
        Key::BarfBackward,
        Key::Character('('),
        Key::Character(')'),
        Key::Character('['),
        Key::Character(']'),
        Key::Character('{'),
        Key::Character('}'),
        Key::Character('\''),
        Key::Character('"'),
        Key::Character('\\'),
        Key::Character('$'),
    ];
    for source in [
        "(list alpha(beta gamma)delta [one {key value}] tail)",
        "(str \"a\\nb\\\"c $(str \"x\" (str y))\" '甲e\u{301}👩‍💻')",
        "(str '''a '' b''' \"\"\"a \"\" b $(str x)\"\"\")",
        "(list (; ignored [a b] 'raw') `(a ,b ,@(list c)))",
        "(list ()[]{} \"\" '' ( ) [\n\t] end)",
        "(list `(a b)c d)",
    ] {
        for cursor in source
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(source.len()))
        {
            for key in keys {
                let mut repl = Repl::new();
                repl.paste(source);
                repl.session.input.cursor = cursor;
                repl.input(key);
                assert!(
                    repl.session
                        .input
                        .body
                        .is_char_boundary(repl.session.input.cursor)
                );
                assert!(
                    readable(&repl.session.input),
                    "{source:?} at {cursor}, {key:?}: {}",
                    state(&repl)
                );
            }
        }
    }
}

#[test]
fn unfinished_pastes_and_depth_limits_allow_safe_recovery() {
    let keys = [
        Key::Backspace,
        Key::Delete,
        Key::KillStart,
        Key::KillEnd,
        Key::KillSexp,
        Key::Transpose,
        Key::WrapSexp,
        Key::SpliceSexp,
        Key::SlurpForward,
        Key::SlurpBackward,
        Key::BarfForward,
        Key::BarfBackward,
        Key::SexpForward,
        Key::SexpBackward,
        Key::SexpUp,
        Key::SexpDown,
        Key::Character('('),
        Key::Character(')'),
        Key::Character('\''),
        Key::Character('"'),
    ];
    let sources = [
        "(str [a}) 甲e\u{301}👩‍💻".to_owned(),
        "str \"$(str 'unfinished".into(),
        "str \"bad\\q and dangling\\".into(),
        "(list `(a ,) '''unfinished ''".into(),
        "#! /bin/parsh\n(str [".into(),
        "#flase".into(),
        format!("{}甲{}", "(".repeat(260), ")".repeat(259)),
    ];
    for source in sources {
        let mut cursors: Vec<_> = source
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(source.len()))
            .collect();
        if source.len() > 256 {
            cursors.retain(|&i| {
                [
                    0,
                    1,
                    254,
                    255,
                    256,
                    257,
                    260,
                    source.len() - 1,
                    source.len(),
                ]
                .contains(&i)
            });
        }
        for cursor in cursors {
            for key in keys {
                let mut repl = Repl::new();
                repl.paste(&source);
                repl.session.input.cursor = cursor;
                let events = repl.input(key);
                assert!(events.iter().all(|e| matches!(e, Event::Render { .. })));
                let input = &repl.session.input;
                assert!(
                    input
                        .body
                        .grapheme_indices(true)
                        .map(|(i, _)| i)
                        .chain(std::iter::once(input.body.len()))
                        .any(|i| i == input.cursor)
                );
            }
        }
    }
}
