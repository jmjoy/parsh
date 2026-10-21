use parsh::{editor, run_source, shell::Shell};
use std::{
    env, fs,
    io::{self, IsTerminal, Read},
    process::ExitCode,
};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code as u8),
        Err(error) => {
            eprintln!("parsh: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let mut arguments = env::args().skip(1);
    let first = arguments.next();
    match first.as_deref() {
        Some("--help" | "-h") => {
            println!(
                "parsh — a shell built around S-expressions\n\nUsage: parsh [FILE [ARGS...]]\n       parsh -c SOURCE [ARGS...]\n\nNo arguments: interactive on a terminal, otherwise read a script from stdin.\nInteractive: ')' submits a complete expression; Enter inserts a newline."
            );
            Ok(0)
        }
        Some("--version" | "-V") => {
            println!("parsh {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        Some("-c") => {
            let source = arguments.next().ok_or("-c requires source code")?;
            let mut shell = Shell::new(arguments.collect())?;
            Ok(run_source(&mut shell, "-c", &source))
        }
        Some("--") => {
            let file = arguments.next().ok_or("-- requires a script path")?;
            let source = fs::read_to_string(&file)?;
            let mut shell = Shell::new(arguments.collect())?;
            Ok(run_source(&mut shell, &file, &source))
        }
        Some(option) if option.starts_with('-') && option != "-" => {
            Err(format!("unknown option {option}; use --help").into())
        }
        Some(file) if file != "-" => {
            let source = fs::read_to_string(file)?;
            let mut shell = Shell::new(arguments.collect())?;
            Ok(run_source(&mut shell, file, &source))
        }
        _ => {
            let mut shell = Shell::new(arguments.collect())?;
            if first.is_none() && io::stdin().is_terminal() {
                shell.interactive()?;
                Ok(editor::run(&mut shell)?)
            } else {
                let mut source = String::new();
                io::stdin().read_to_string(&mut source)?;
                Ok(run_source(&mut shell, "stdin", &source))
            }
        }
    }
}
