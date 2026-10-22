#[cfg(feature = "native")]
pub mod editor;
pub mod host;
#[cfg(feature = "native")]
pub mod job;
#[cfg(feature = "native")]
pub mod process;
pub mod reader;
pub mod repl;
pub mod shell;
mod streams;

use reader::{Diagnostic, Kind, Read, Reader};
use shell::Shell;

/// Execute incrementally, recovering extra ')' one character at a time.
/// A runtime failure disables further evaluation, but the reader keeps diagnosing.
pub fn run_source(shell: &mut Shell, name: &str, source: &str) -> i32 {
    let mut reader = Reader::new(source);
    let mut halted = false;
    let mut code = 0;
    loop {
        match reader.read() {
            Read::End => return code,
            Read::Form(expr) => {
                if halted || expr.kind == Kind::Comment {
                    continue;
                }
                if !matches!(expr.kind, Kind::List(_)) {
                    eprintln!(
                        "{}",
                        Diagnostic {
                            offset: expr.offset,
                            message: "top-level expressions must be parenthesized calls".into()
                        }
                        .render(name, source)
                    );
                    code = 2;
                    halted = true;
                    continue;
                }
                if let Err(error) = shell.execute(&expr) {
                    if error.exit {
                        return error.code;
                    }
                    eprintln!(
                        "{}",
                        Diagnostic {
                            offset: error.offset,
                            message: error.message
                        }
                        .render(name, source)
                    );
                    code = error.code;
                    halted = true;
                }
            }
            Read::ExtraClose(diagnostic) => {
                eprintln!("{}", diagnostic.render(name, source));
                if code == 0 {
                    code = 2;
                }
            }
            Read::Incomplete(diagnostic) | Read::Error(diagnostic) => {
                eprintln!("{}", diagnostic.render(name, source));
                return if code == 0 { 2 } else { code };
            }
        }
    }
}
