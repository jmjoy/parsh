use std::io::{self, Write};

use crate::{
    repl::{ConsoleBackend, Input, Key, Session, layout},
    shell::Shell,
};
use crossterm::{
    cursor::{MoveDown, MoveToColumn, MoveUp},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute, queue,
    style::{Color, Print, SetForegroundColor, force_color_output},
    terminal::{self, Clear, ClearType, DisableLineWrap, EnableLineWrap},
};

struct TerminalConsole {
    output: io::Stderr,
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

    fn draw(&mut self, input: &Input) -> io::Result<()> {
        let (width, height) = terminal::size().unwrap_or((80, 24));
        let (lines, cursor) = layout(input, usize::from(width.max(2)));
        let available = usize::from(height.max(3) - 1);
        let first = cursor.0.saturating_sub(available - 1);
        let last = lines.len().min(first + available);
        queue!(self.output, MoveToColumn(0))?;
        if self.cursor_row > 0 {
            queue!(self.output, MoveUp(self.cursor_row as u16))?;
        }
        queue!(self.output, Clear(ClearType::FromCursorDown))?;
        let dim_prompt = !input.explicit && input.body.trim().is_empty();
        for (index, line) in lines[first..last].iter().enumerate() {
            if index != 0 {
                queue!(self.output, Print("\r\n"))?;
            }
            if dim_prompt && first == 0 && index == 0 {
                queue!(
                    self.output,
                    SetForegroundColor(Color::AnsiValue(244)),
                    Print("("),
                    SetForegroundColor(Color::Reset),
                    Print(&line[1..])
                )?;
            } else {
                queue!(self.output, Print(line))?;
            }
        }
        self.rows = last - first;
        self.cursor_row = cursor.0 - first;
        let up = self.rows - 1 - self.cursor_row;
        if up > 0 {
            queue!(self.output, MoveUp(up as u16))?;
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
    fn draw(&mut self, input: &Input) -> io::Result<()> {
        TerminalConsole::draw(self, input)
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
            Event::Paste(text) => session.paste(shell, &mut console, &text)?,
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let mapped = if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('c') => Some(Key::Interrupt),
                        KeyCode::Char('d') => Some(Key::Eof),
                        KeyCode::Char('a') => Some(Key::Home),
                        KeyCode::Char('e') => Some(Key::End),
                        KeyCode::Char('p') => Some(Key::Previous),
                        KeyCode::Char('n') => Some(Key::Next),
                        KeyCode::Char('u') => Some(Key::KillStart),
                        KeyCode::Char('k') => Some(Key::KillEnd),
                        _ => None,
                    }
                } else {
                    match key.code {
                        KeyCode::Char(ch) => Some(Key::Character(ch)),
                        KeyCode::Enter => Some(Key::Enter),
                        KeyCode::Tab => Some(Key::Tab),
                        KeyCode::Left => Some(Key::Left),
                        KeyCode::Right => Some(Key::Right),
                        KeyCode::Backspace => Some(Key::Backspace),
                        KeyCode::Delete => Some(Key::Delete),
                        KeyCode::Home => Some(Key::Home),
                        KeyCode::End => Some(Key::End),
                        KeyCode::Up => Some(Key::Up),
                        KeyCode::Down => Some(Key::Down),
                        _ => None,
                    }
                };
                match mapped {
                    Some(key) => session.key(shell, &mut console, key)?,
                    None => None,
                }
            }
            _ => None,
        };
        if let Some(code) = code {
            return Ok(code);
        }
    }
}
