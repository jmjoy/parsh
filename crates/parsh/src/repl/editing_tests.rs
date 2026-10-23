use super::*;

fn type_text(repl: &mut Repl, text: &str) -> Vec<Event> {
    text.chars()
        .flat_map(|ch| repl.input(Key::Character(ch)))
        .collect()
}

fn edited(events: &[Event]) {
    assert!(
        events
            .iter()
            .all(|event| matches!(event, Event::Render { .. })),
        "{events:?}"
    );
}

fn render(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Render { lines, .. } => Some(lines.join("\n")),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn word_movement_and_killing_distinguish_punctuation_from_whitespace() {
    let mut repl = Repl::new();
    let text = "str foo_bar-baz/甲e\u{301}.txt \t";
    repl.paste(text);
    repl.input(Key::WordLeft);
    assert_eq!(
        &repl.session.input.body[repl.session.input.cursor..],
        "txt \t"
    );
    repl.input(Key::WordLeft);
    assert_eq!(
        &repl.session.input.body[repl.session.input.cursor..],
        "甲e\u{301}.txt \t"
    );
    repl.input(Key::WordRight);
    assert_eq!(
        &repl.session.input.body[repl.session.input.cursor..],
        ".txt \t"
    );
    repl.input(Key::End);
    repl.input(Key::KillWordBack);
    assert_eq!(repl.session.input.body, "str foo_bar-baz/甲e\u{301}.");
    repl.input(Key::KillWordBack);
    assert_eq!(repl.session.input.body, "str foo_bar-baz/");
    repl.input(Key::Yank);
    assert_eq!(repl.session.input.body, text);
    repl.input(Key::KillWhitespaceWord);
    assert_eq!(repl.session.input.body, "str ");
    repl.input(Key::Yank);
    assert_eq!(repl.session.input.body, text);
}

#[test]
fn kill_ring_coalesces_in_both_directions_and_escape_y_rotates() {
    let mut repl = Repl::new();
    repl.paste("one two");
    repl.input(Key::KillWhitespaceWord);
    repl.input(Key::Home); // A movement starts a separate kill.
    repl.input(Key::End);
    repl.input(Key::KillWhitespaceWord);
    repl.input(Key::Yank);
    assert_eq!(repl.session.input.body, "one ");
    repl.input(Key::Escape);
    repl.input(Key::Character('y'));
    assert_eq!(repl.session.input.body, "two");
    repl.input(Key::YankPop);
    assert_eq!(repl.session.input.body, "one ");
    repl.input(Key::Left);
    repl.input(Key::YankPop);
    assert_eq!(repl.session.input.body, "one ");

    repl.input(Key::ClearInput);
    repl.paste("one two three");
    repl.input(Key::Home);
    repl.input(Key::KillWordForward);
    repl.input(Key::Escape);
    repl.input(Key::Character('d'));
    assert_eq!(repl.session.input.body, " three");
    repl.input(Key::Yank);
    assert_eq!(repl.session.input.body, "one two three");
}

#[test]
fn line_kills_include_a_newline_only_when_at_the_line_end() {
    let mut repl = Repl::new();
    repl.paste("str first\nsecond");
    repl.input(Key::Home);
    repl.input(Key::Left);
    repl.input(Key::KillEnd);
    assert_eq!(repl.session.input.body, "str firstsecond");
    repl.input(Key::Yank);
    assert_eq!(repl.session.input.body, "str first\nsecond");
    repl.input(Key::End);
    repl.input(Key::KillStart);
    assert_eq!(repl.session.input.body, "str first\n");
    repl.input(Key::Yank);
    assert_eq!(repl.session.input.body, "str first\nsecond");
}

#[test]
fn transpose_and_character_edits_preserve_whole_graphemes() {
    let mut repl = Repl::new();
    repl.paste("甲e\u{301}👩‍💻乙");
    repl.input(Key::Transpose);
    assert_eq!(repl.session.input.body, "甲e\u{301}乙👩‍💻");
    repl.input(Key::Undo);
    repl.input(Key::Left);
    repl.input(Key::Transpose);
    assert_eq!(repl.session.input.body, "甲e\u{301}乙👩‍💻");
    repl.input(Key::Backspace);
    assert_eq!(repl.session.input.body, "甲e\u{301}乙");
    repl.input(Key::Home);
    repl.input(Key::Transpose);
    assert_eq!(repl.session.input.body, "甲e\u{301}乙");
    repl.input(Key::Right);
    repl.input(Key::Delete);
    assert_eq!(repl.session.input.body, "甲乙");
}

#[test]
fn edits_that_join_graphemes_leave_the_cursor_on_a_boundary() {
    let mut repl = Repl::new();
    repl.paste("eX\u{301}");
    repl.input(Key::Home);
    repl.input(Key::Right);
    repl.input(Key::Delete);
    assert_eq!(repl.session.input.body, "e");
    // Inserting before a leading combining mark joins it to the inserted text.
    repl.input(Key::ClearInput);
    repl.paste("\u{301}");
    repl.input(Key::Home);
    type_text(&mut repl, "e");
    assert_eq!(repl.session.input.body, "e\u{301}");
    assert_eq!(repl.session.input.cursor, repl.session.input.body.len());
    repl.input(Key::Backspace);
    assert!(repl.session.input.body.is_empty());
}

#[test]
fn undo_groups_typing_and_restores_cursor_and_explicit_mode() {
    let mut repl = Repl::new();
    type_text(&mut repl, "(str 甲e\u{301}👩‍💻");
    repl.input(Key::Undo);
    assert!(repl.session.input.body.is_empty());
    assert!(!repl.session.input.explicit);
    repl.paste("str alpha");
    repl.input(Key::Left);
    let cursor = repl.session.input.cursor;
    type_text(&mut repl, "XYZ");
    repl.input(Key::ControlX);
    repl.input(Key::KillStart);
    assert_eq!(repl.session.input.body, "str alpha");
    assert_eq!(repl.session.input.cursor, cursor);
    repl.input(Key::Undo);
    assert!(repl.session.input.body.is_empty());
}

#[test]
fn completion_paste_deletion_and_yank_are_separate_undo_groups() {
    let mut repl = Repl::new();
    type_text(&mut repl, "li");
    repl.input(Key::Tab);
    assert_ne!(repl.session.input.body, "li");
    repl.input(Key::Undo);
    assert_eq!(repl.session.input.body, "li");
    repl.input(Key::Undo);
    assert!(repl.session.input.body.is_empty());
    repl.paste("str first");
    repl.paste(" second");
    repl.input(Key::Undo);
    assert_eq!(repl.session.input.body, "str first");
    repl.input(Key::Backspace);
    repl.input(Key::Undo);
    assert_eq!(repl.session.input.body, "str first");
    repl.input(Key::KillWhitespaceWord);
    repl.input(Key::Yank);
    repl.input(Key::Undo);
    assert_eq!(repl.session.input.body, "str ");
    repl.input(Key::Undo);
    assert_eq!(repl.session.input.body, "str first");
}

#[test]
fn yanking_and_undoing_complete_source_never_submit() {
    let mut repl = Repl::new();
    type_text(&mut repl, "str safeZ");
    repl.input(Key::Left);
    edited(&repl.paste(")"));
    repl.input(Key::Delete);
    assert_eq!(repl.session.input.source(), "(str safe)");
    repl.input(Key::KillStart);
    edited(&repl.input(Key::Yank));
    assert_eq!(repl.session.input.source(), "(str safe)");
    repl.input(Key::Undo);
    edited(&repl.input(Key::Undo));
    assert_eq!(repl.session.input.source(), "(str safe)");
    assert!(repl.session.history.entries.is_empty());
    assert!(
        repl.input(Key::Character(')'))
            .contains(&Event::Value("\"safe\"".into()))
    );
}

#[test]
fn submissions_interrupts_and_history_recall_start_undo_baselines() {
    let mut repl = Repl::new();
    type_text(&mut repl, "str first");
    repl.input(Key::KillWhitespaceWord);
    repl.input(Key::Yank);
    type_text(&mut repl, ")");
    repl.input(Key::Undo);
    assert!(repl.session.input.body.is_empty());
    repl.input(Key::Yank); // The kill ring survives submission.
    assert_eq!(repl.session.input.body, "first");
    repl.input(Key::Interrupt);
    repl.input(Key::Undo);
    assert!(repl.session.input.body.is_empty());
    repl.input(Key::Previous);
    repl.input(Key::Undo);
    assert_eq!(repl.session.input.source(), "(str first");
    type_text(&mut repl, " second");
    repl.input(Key::Undo);
    assert_eq!(repl.session.input.source(), "(str first");
    repl.reset();
    repl.input(Key::Yank);
    assert!(repl.session.input.body.is_empty());
}

#[test]
fn paste_submission_and_preserved_middle_tail_do_not_restore_executed_text() {
    let mut repl = Repl::new();
    repl.paste("(str first) (str unfinished");
    repl.input(Key::Undo);
    assert!(repl.session.input.body.is_empty());
    type_text(&mut repl, "str firsttail");
    for _ in 0..4 {
        repl.input(Key::Left);
    }
    type_text(&mut repl, ")");
    repl.input(Key::Undo);
    assert_eq!(repl.session.input.body, "tail");
    type_text(&mut repl, "str ");
    repl.input(Key::Undo);
    assert_eq!(repl.session.input.body, "tail");
    assert_eq!(repl.session.input.cursor, 0);
}

#[test]
fn search_walks_both_directions_and_accepts_without_execution() {
    let mut repl = Repl::new();
    repl.paste("(str first)(str second)(str first-again)");
    type_text(&mut repl, "str draft");
    repl.input(Key::SearchBackward);
    let events = type_text(&mut repl, "first");
    edited(&events);
    assert!(render(&events).contains("(str first-again"));
    let events = repl.input(Key::SearchBackward);
    assert!(render(&events).starts_with("(str first\n"));
    assert!(render(&repl.input(Key::SearchForward)).contains("(str first-again"));
    assert!(render(&type_text(&mut repl, "missing")).contains("failed forward-i-search"));
    assert!(render(&repl.resize(80, 24)).contains("failed forward-i-search"));
    edited(&repl.input(Key::Enter));
    assert_eq!(repl.session.input.source(), "(str first-again");
    repl.input(Key::Undo); // Search acceptance is a fresh editing baseline.
    assert_eq!(repl.session.input.source(), "(str first-again");
    assert!(
        repl.input(Key::Character(')'))
            .contains(&Event::Value("\"first-again\"".into()))
    );
}

#[test]
fn abort_search_restores_draft_cursor_mode_and_undo_history() {
    for draft in ["str saved", "(str saved"] {
        let mut repl = Repl::new();
        repl.paste("(str 甲e\u{301}👩‍💻)");
        type_text(&mut repl, draft);
        repl.input(Key::Left);
        let before = repl.session.input.clone();
        repl.input(Key::SearchBackward);
        type_text(&mut repl, "甲e\u{301}👩‍💻");
        repl.input(Key::Backspace);
        assert!(render(&repl.resize(80, 24)).contains("甲e\u{301}':"));
        repl.input(Key::Abort);
        assert!(repl.session.input == before);
        repl.input(Key::Undo);
        assert!(repl.session.input.body.is_empty());
    }
}

#[test]
fn search_paste_is_literal_and_empty_queries_and_failures_are_safe() {
    let mut repl = Repl::new();
    repl.paste("(str retained)");
    type_text(&mut repl, "str draft");
    repl.input(Key::SearchBackward);
    edited(&repl.paste("(exit 9)\n"));
    assert_eq!(repl.session.history.entries.len(), 1);
    assert!(render(&repl.resize(80, 24)).contains("failed reverse-i-search"));
    repl.input(Key::Escape);
    assert_eq!(repl.session.input.source(), "(str draft");
    // Escape used to accept a search must not consume the next typed character.
    type_text(&mut repl, "X");
    assert_eq!(repl.session.input.body, "str draftX");
    repl.input(Key::SearchBackward);
    repl.input(Key::Enter);
    assert_eq!(repl.session.input.body, "str draftX");
}

#[test]
fn search_multiline_unicode_resize_clear_and_history_draft_restore() {
    let mut repl = Repl::new();
    repl.paste("(str\n  甲e\u{301}👩‍💻)");
    type_text(&mut repl, "str draft");
    repl.input(Key::SearchBackward);
    type_text(&mut repl, "甲e\u{301}👩‍💻");
    let events = repl.resize(8, 4);
    assert!(
        matches!(events.last(), Some(Event::Render { lines, row, column, dim_prompt: false })
        if lines.len() <= 3 && *row < lines.len() && *column < 8)
    );
    let events = repl.input(Key::ClearScreen);
    assert!(matches!(events.first(), Some(Event::ClearScreen)));
    assert!(repl.session.search.is_some());
    repl.input(Key::Enter);
    assert_eq!(repl.session.input.source(), "(str\n  甲e\u{301}👩‍💻");
    repl.input(Key::Next);
    assert_eq!(repl.session.input.source(), "(str draft");
    assert!(!repl.session.input.explicit);
}

#[test]
fn search_editing_accepts_preview_and_interrupt_preserves_exit_status() {
    let mut repl = Repl::new();
    repl.paste("(str target)");
    repl.input(Key::SearchBackward);
    type_text(&mut repl, "target");
    repl.input(Key::Home);
    assert_eq!(repl.session.input.source(), "(str target");
    assert_eq!(repl.session.input.cursor, 0);
    repl.input(Key::SearchBackward);
    type_text(&mut repl, "target");
    repl.input(Key::Interrupt);
    assert!(repl.session.search.is_none());
    assert!(repl.session.input.body.is_empty());
    assert!(repl.input(Key::Eof).contains(&Event::Exit(130)));
}

#[test]
fn unsupported_and_aborted_prefixes_do_not_modify_the_buffer() {
    let mut repl = Repl::new();
    type_text(&mut repl, "str draft");
    repl.input(Key::ControlX);
    type_text(&mut repl, "q");
    assert_eq!(repl.session.input.body, "str draft");
    repl.input(Key::Escape);
    repl.input(Key::Abort);
    type_text(&mut repl, "x");
    assert_eq!(repl.session.input.body, "str draftx");
    repl.input(Key::Escape);
    repl.input(Key::Ignore);
    type_text(&mut repl, "y");
    assert_eq!(repl.session.input.body, "str draftxy");
}

#[test]
fn yank_rotation_restores_the_original_implicit_prompt_mode() {
    let mut repl = Repl::new();
    type_text(&mut repl, "str implicit");
    repl.input(Key::KillStart);
    type_text(&mut repl, "(str explicit");
    repl.input(Key::KillStart);
    repl.input(Key::Yank);
    assert_eq!(repl.session.input.source(), "(str explicit");
    assert!(repl.session.input.explicit);
    repl.input(Key::YankPop);
    assert_eq!(repl.session.input.source(), "(str implicit");
    assert!(!repl.session.input.explicit);
    repl.input(Key::Undo);
    assert!(repl.session.input.explicit);
}

#[test]
fn empty_and_single_grapheme_edits_are_no_ops_and_last_search_can_be_reused() {
    let mut repl = Repl::new();
    for key in [
        Key::Transpose,
        Key::KillEnd,
        Key::KillStart,
        Key::KillWordBack,
        Key::KillWordForward,
        Key::KillWhitespaceWord,
        Key::Yank,
        Key::YankPop,
        Key::Undo,
    ] {
        edited(&repl.input(key));
        assert!(repl.session.input.body.is_empty());
    }
    repl.paste("👩‍💻");
    repl.input(Key::Transpose);
    assert_eq!(repl.session.input.body, "👩‍💻");
    repl.input(Key::ClearInput);
    repl.paste("(str TARGET)");
    repl.input(Key::SearchBackward);
    type_text(&mut repl, "TARGET");
    repl.input(Key::Abort);
    repl.input(Key::SearchBackward);
    assert!(render(&repl.input(Key::SearchBackward)).contains("(str TARGET"));
    repl.input(Key::Enter);
    assert_eq!(repl.session.input.source(), "(str TARGET");
}

#[test]
fn narrowing_a_search_after_switching_direction_includes_the_current_match() {
    let mut repl = Repl::new();
    repl.paste("(str first)(str second)(str first-again)");
    repl.input(Key::SearchBackward);
    type_text(&mut repl, "fir");
    repl.input(Key::SearchBackward);
    repl.input(Key::SearchForward);
    let events = type_text(&mut repl, "st");
    assert!(!render(&events).contains("failed"));
    repl.input(Key::Enter);
    assert_eq!(repl.session.input.source(), "(str first-again");
}
