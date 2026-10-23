use std::io::{self, Write};

use crate::{
    repl::{ConsoleBackend, Input, Key, Session, completion_lines, view_layout},
    shell::Shell,
};
use crossterm::{
    cursor::{MoveDown, MoveTo, MoveToColumn, MoveUp},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute, queue,
    style::{Color, Print, SetForegroundColor, force_color_output},
    terminal::{self, Clear, ClearType, DisableLineWrap, EnableLineWrap},
};
use nix::unistd::geteuid;

struct TerminalConsole {
    output: io::Stderr,
    prefix: &'static str,
    active: bool,
    rows: usize,
    cursor_row: usize,
}

impl TerminalConsole {
    fn new() -> io::Result<Self> {
        // Prompt color is an editing cue; keep it even when commands use NO_COLOR.
        force_color_output(true);
        let mut console = Self {
            output: io::stderr(),
            prefix: if geteuid().is_root() { "# " } else { "$ " },
            active: false,
            rows: 0,
            cursor_row: 0,
        };
        console.resume()?;
        Ok(console)
    }

    fn resume(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        self.active = true;
        execute!(self.output, EnableBracketedPaste, DisableLineWrap)
    }

    fn pause(&mut self) -> io::Result<()> {
        self.finish()?;
        if self.active {
            execute!(self.output, DisableBracketedPaste, EnableLineWrap)?;
            terminal::disable_raw_mode()?;
            self.active = false;
        }
        Ok(())
    }

    fn draw(&mut self, input: &Input, search: Option<&str>) -> io::Result<()> {
        let (width, height) = terminal::size().unwrap_or((80, 24));
        let view = view_layout(
            input,
            search,
            self.prefix,
            usize::from(width),
            usize::from(height),
        );
        let lines = &view.lines;
        let cursor = view.cursor;
        queue!(self.output, MoveToColumn(0))?;
        if self.cursor_row > 0 {
            queue!(self.output, MoveUp(self.cursor_row as u16))?;
        }
        queue!(self.output, Clear(ClearType::FromCursorDown))?;
        for (index, line) in lines.iter().enumerate() {
            if index != 0 {
                queue!(self.output, Print("\r\n"))?;
            }
            queue!(self.output, Print(line))?;
        }
        self.rows = lines.len();
        self.cursor_row = cursor.0;
        let mut drawn_row = self.rows - 1;
        for cell in view.dimmed_cells {
            if drawn_row > cell.row {
                queue!(self.output, MoveUp((drawn_row - cell.row) as u16))?;
            } else if cell.row > drawn_row {
                queue!(self.output, MoveDown((cell.row - drawn_row) as u16))?;
            }
            queue!(
                self.output,
                MoveToColumn(cell.column as u16),
                SetForegroundColor(Color::AnsiValue(244)),
                Print(cell.character),
                SetForegroundColor(Color::Reset)
            )?;
            drawn_row = cell.row;
        }
        if drawn_row > cursor.0 {
            queue!(self.output, MoveUp((drawn_row - cursor.0) as u16))?;
        } else if cursor.0 > drawn_row {
            queue!(self.output, MoveDown((cursor.0 - drawn_row) as u16))?;
        }
        queue!(self.output, MoveToColumn(cursor.1 as u16))?;
        self.output.flush()
    }

    fn finish(&mut self) -> io::Result<()> {
        if self.rows > 0 {
            let down = self.rows - 1 - self.cursor_row;
            if down > 0 {
                queue!(self.output, MoveDown(down as u16))?;
            }
            queue!(self.output, MoveToColumn(0), Print("\r\n"))?;
            self.rows = 0;
            self.cursor_row = 0;
            self.output.flush()?;
        }
        Ok(())
    }
}

impl Drop for TerminalConsole {
    fn drop(&mut self) {
        if self.active {
            let _ = execute!(self.output, DisableBracketedPaste, EnableLineWrap);
            let _ = terminal::disable_raw_mode();
        }
    }
}

impl ConsoleBackend for TerminalConsole {
    fn draw(&mut self, input: &Input, search: Option<&str>) -> io::Result<()> {
        TerminalConsole::draw(self, input, search)
    }
    fn clear_screen(&mut self) -> io::Result<()> {
        execute!(self.output, Clear(ClearType::All), MoveTo(0, 0))?;
        self.rows = 0;
        self.cursor_row = 0;
        Ok(())
    }
    fn completions(&mut self, candidates: &[String]) -> io::Result<()> {
        self.finish()?;
        let (width, _) = terminal::size().unwrap_or((80, 24));
        for line in completion_lines(candidates, usize::from(width)) {
            queue!(self.output, Print(line), Print("\r\n"))?;
        }
        self.output.flush()
    }
    fn pause(&mut self) -> io::Result<()> {
        TerminalConsole::pause(self)
    }
    fn resume(&mut self) -> io::Result<()> {
        TerminalConsole::resume(self)
    }
    fn value(&mut self, text: &str) -> io::Result<()> {
        println!("{text}");
        Ok(())
    }
    fn diagnostic(&mut self, text: &str) -> io::Result<()> {
        eprintln!("{text}");
        Ok(())
    }
}

pub fn run(shell: &mut Shell) -> io::Result<i32> {
    let mut console = TerminalConsole::new()?;
    let mut session = Session::default();
    loop {
        session.draw(&mut console)?;
        let code = match event::read()? {
            Event::Paste(text) => {
                session.paste(&text);
                None
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                session.key(shell, &mut console, map_key(key))?
            }
            _ => None,
        };
        if let Some(code) = code {
            return Ok(code);
        }
    }
}

fn map_key(key: KeyEvent) -> Key {
    if key
        .modifiers
        .intersects(KeyModifiers::SUPER | KeyModifiers::HYPER | KeyModifiers::META)
    {
        return Key::Ignore;
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        return match (key.code, key.modifiers.contains(KeyModifiers::CONTROL)) {
            (KeyCode::Char('b'), false) => Key::WordLeft,
            (KeyCode::Char('f'), false) => Key::WordRight,
            (KeyCode::Char('d'), false) => Key::KillWordForward,
            (KeyCode::Char('y'), false) => Key::YankPop,
            (KeyCode::Backspace, _) | (KeyCode::Char('h'), true) => Key::KillWordBack,
            _ => Key::Ignore,
        };
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Key::Interrupt,
            KeyCode::Char('d') => Key::Eof,
            KeyCode::Char('a') => Key::Home,
            KeyCode::Char('e') => Key::End,
            KeyCode::Char('b') => Key::Left,
            KeyCode::Char('f') => Key::Right,
            KeyCode::Char('h') | KeyCode::Backspace => Key::Backspace,
            KeyCode::Char('p') => Key::Previous,
            KeyCode::Char('n') => Key::Next,
            KeyCode::Char('u') => Key::KillStart,
            KeyCode::Char('k') => Key::KillEnd,
            KeyCode::Char('w') => Key::KillWhitespaceWord,
            KeyCode::Char('y') => Key::Yank,
            KeyCode::Char('t') => Key::Transpose,
            KeyCode::Char('l') => Key::ClearScreen,
            KeyCode::Char('r') => Key::SearchBackward,
            KeyCode::Char('s') => Key::SearchForward,
            KeyCode::Char('g') => Key::Abort,
            KeyCode::Char('x') => Key::ControlX,
            // Crossterm's legacy parser reports byte 0x1f as Ctrl+7.
            KeyCode::Char('_' | '/' | '7') => Key::Undo,
            KeyCode::Char('j') => Key::Newline,
            KeyCode::Char('m') | KeyCode::Enter => Key::Enter,
            KeyCode::Char('i') | KeyCode::Tab => Key::Tab,
            _ => Key::Ignore,
        };
    }
    match key.code {
        KeyCode::Char(ch) => Key::Character(ch),
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Esc => Key::Escape,
        _ => Key::Ignore,
    }
}
