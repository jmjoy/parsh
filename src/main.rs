use clap::Parser;
use parsh::{editor, run_source, shell::Shell};
use std::{
    fs,
    io::{self, IsTerminal, Read},
    process::ExitCode,
};

#[derive(Parser)]
#[command(
    version,
    about,
    after_help = "With no FILE or -c, start interactively on a terminal, otherwise read a script from stdin.\nInteractive: ')' submits a complete expression; Enter inserts a newline."
)]
struct Cli {
    /// Execute SOURCE, with remaining arguments available as $args
    #[arg(
        short = 'c',
        value_names = ["SOURCE", "ARGS"],
        num_args = 1..,
        allow_hyphen_values = true,
        conflicts_with = "script"
    )]
    command: Option<Vec<String>>,

    /// Script path ('-' reads stdin), followed by arguments available as $args
    #[arg(value_names = ["FILE", "ARGS"], num_args = 1.., trailing_var_arg = true)]
    script: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => ExitCode::from(code as u8),
        Err(error) => {
            eprintln!("parsh: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<i32, Box<dyn std::error::Error>> {
    if let Some(command) = cli.command {
        let mut arguments = command.into_iter();
        let source = arguments.next().expect("clap requires SOURCE with -c");
        let mut shell = Shell::new(arguments.collect())?;
        return Ok(run_source(&mut shell, "-c", &source));
    }

    let mut arguments = cli.script.into_iter();
    let first = arguments.next();
    match first.as_deref() {
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
