use super::*;
use crate::reader::{Kind as ExprKind, Read, Reader};

#[test]
fn context_tracks_lists_comments_and_declarations() {
    for (source, kind, prefix) in [
        ("(", Kind::Command, ""),
        ("(st", Kind::Command, "st"),
        ("(do\n (str (li", Kind::Command, "li"),
        ("((; ignored) st", Kind::Command, "st"),
        (
            "(cd su",
            Kind::Path {
                directories_only: true,
            },
            "su",
        ),
        (
            "(cd (; ignored) su",
            Kind::Path {
                directories_only: true,
            },
            "su",
        ),
        (
            "(str su",
            Kind::Path {
                directories_only: false,
            },
            "su",
        ),
        ("(str $HO", Kind::Variable, "$HO"),
        ("(str \"$HO", Kind::Variable, "$HO"),
        ("(str \"$(st", Kind::Command, "st"),
        ("(str \"$(str \"$HO", Kind::Variable, "$HO"),
        ("(defn f (name) (st", Kind::Command, "st"),
        (
            "(do (; ignored (nested)) (cd su",
            Kind::Path {
                directories_only: true,
            },
            "su",
        ),
    ] {
        let context = Context::parse(source, source.len()).unwrap();
        assert_eq!(
            (context.kind, context.prefix.as_str()),
            (kind, prefix),
            "{source}"
        );
    }
    for source in [
        "(let na",
        "(set na",
        "(export NA",
        "(for it",
        "(defn fu",
        "(defn f (na",
        "(;",
        "(; nested (st",
        "((; ignored) ; more (st",
        "(str \"bad\\q",
        "(str \"unfinished\\",
        "(str x) ",
    ] {
        assert!(Context::parse(source, source.len()).is_none(), "{source}");
    }
}

#[test]
fn quoted_paths_decode_only_reader_supported_escapes() {
    let source = "(str \"a\\\"b\\\\c\\n\\r\\t";
    let context = Context::parse(source, source.len()).unwrap();
    assert_eq!(context.prefix, "a\"b\\c\n\r\t");
    assert_eq!(context.quote, Quote::Double);
    assert!(!context.closed);
}

#[test]
fn nested_interpolation_contexts_preserve_outer_string_boundaries() {
    for (marked, kind, prefix) in [
        (
            r#"(str "prefix $HO|ME/suffix" tail)"#,
            Kind::Variable,
            "$HO",
        ),
        (
            r#"(str "prefix $(st|r "inner") suffix" tail)"#,
            Kind::Command,
            "st",
        ),
        (r#"(str "$(str "$(ge|t name)")")"#, Kind::Command, "ge"),
        (
            r#"(str 'a\n $HO|')"#,
            Kind::Path {
                directories_only: false,
            },
            r#"a\n $HO"#,
        ),
        (
            r#"(str "\$HO|")"#,
            Kind::Path {
                directories_only: false,
            },
            "$HO",
        ),
        (
            r#"(cd "$(str x)" fo|)"#,
            Kind::Path {
                directories_only: false,
            },
            "fo",
        ),
    ] {
        let cursor = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        let context = Context::parse(&source, cursor).unwrap();
        assert_eq!(
            (context.kind, context.prefix.as_str()),
            (kind, prefix),
            "{marked}"
        );
    }
    for marked in [
        r#"(str "$HOME/fo|")"#,
        r#"(str "fo|$(echo later)")"#,
        r#"(; "$(st|r x)")"#,
        r#"(str "$(; (st|r x))")"#,
    ] {
        let cursor = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        assert!(Context::parse(&source, cursor).is_none(), "{marked}");
    }
}

#[test]
fn every_cursor_boundary_in_unfinished_source_is_safe() {
    for source in [
        "(str \"你好 👩‍💻 e\u{301}\" tail)",
        "(do (; \"quotes\\\"\" (nested)) (str $name))",
        "(str \"unterminated\\",
        r#"(str "prefix $名字 $(str "inner $HOME" '(a b)) suffix" tail)"#,
        r#"(str 'raw\n $HOME $(str "inner")' tail)"#,
        "(str ) extra))",
        "(\"\")",
        "\"",
        "",
    ] {
        for (cursor, _) in source
            .char_indices()
            .chain(std::iter::once((source.len(), '\0')))
        {
            if let Some(context) = Context::parse(source, cursor) {
                assert!(context.range.start <= cursor);
                assert!(context.range.end >= cursor);
                assert!(source.is_char_boundary(context.range.start));
                assert!(source.is_char_boundary(context.range.end));
            }
        }
    }
}

#[test]
fn completed_literals_round_trip_through_the_reader() {
    for text in [
        "a file",
        "a(b)",
        "a\"b",
        "a'b",
        "$literal",
        "$(echo unexpected)",
        "a $literal file",
        ";",
        "a\nb\tc\rd\\e",
        "你好👩‍💻",
    ] {
        let source = "(str ";
        let completion = Completion {
            context: Context::parse(source, source.len()).unwrap(),
            candidates: vec![Candidate::word(text)],
        };
        let edit = completion.edit(source).unwrap();
        let completed = format!("{source}{})", edit.text);
        let Read::Form(expr) = Reader::new(&completed).read() else {
            panic!("{completed}")
        };
        let ExprKind::List(items) = expr.kind else {
            panic!()
        };
        assert_eq!(items.len(), 2, "{completed}");
        assert!(
            matches!(&items[1].kind, ExprKind::Text(value) | ExprKind::Word(value) if value == text),
            "{completed}"
        );
        assert_eq!(edit.cursor, edit.text.len());
        assert!(!completion.labels()[0].chars().any(char::is_control));
    }
}

#[cfg(feature = "native")]
mod native {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    fn names(shell: &mut Shell, source: &str) -> Vec<String> {
        Completion::query(shell, source, source.len())
            .unwrap()
            .candidates
            .into_iter()
            .map(|candidate| candidate.text)
            .collect()
    }

    #[test]
    fn path_search_uses_session_cwd_path_and_executable_permissions() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("bin")).unwrap();
        fs::create_dir(directory.path().join("bin/tab-directory")).unwrap();
        let executable = directory.path().join("bin/tab-executable");
        fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(directory.path().join("bin/tab-data"), "data").unwrap();
        symlink(&executable, directory.path().join("bin/tab-link")).unwrap();
        let mut shell = Shell::new(Vec::new()).unwrap();
        shell.cwd = directory.path().into();
        shell
            .environment
            .insert("PATH".into(), "missing:bin:bin".into());
        assert_eq!(names(&mut shell, "(tab-"), ["tab-executable", "tab-link"]);
        assert_eq!(
            names(&mut shell, "(./bin/tab-"),
            [
                "./bin/tab-directory/",
                "./bin/tab-executable",
                "./bin/tab-link"
            ]
        );
        assert_eq!(
            names(&mut shell, "(str bin/tab-"),
            [
                "bin/tab-data",
                "bin/tab-directory/",
                "bin/tab-executable",
                "bin/tab-link"
            ]
        );
        assert_eq!(names(&mut shell, "(cd bin/tab-"), ["bin/tab-directory/"]);
        shell.environment.insert("PATH".into(), ":missing".into());
        fs::copy(&executable, directory.path().join("tab-local")).unwrap();
        assert_eq!(names(&mut shell, "(tab-"), ["tab-local"]);
        assert!(names(&mut shell, "(str missing/").is_empty());
    }

    #[test]
    fn non_utf8_and_terminal_control_filenames_are_not_inserted_lossily() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join(OsString::from_vec(vec![b'a', 255])),
            "",
        )
        .unwrap();
        fs::write(directory.path().join("a\x1b[2J"), "").unwrap();
        fs::write(directory.path().join("allowed"), "").unwrap();
        let mut shell = Shell::new(Vec::new()).unwrap();
        shell.cwd = directory.path().into();
        assert_eq!(names(&mut shell, "(str a"), ["allowed"]);
    }
}
