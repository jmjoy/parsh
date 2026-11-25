use clap::Parser;
use parsh::{editor, run_source, shell::Shell};
use std::{
    fs,
    io::{self, IsTerminal, Read},
    path::PathBuf,
    process::ExitCode,
};

mod startup;

#[derive(Parser)]
#[command(
    version,
    about,
    after_help = "With no FILE or -c, start interactively on a terminal, otherwise read a script from stdin.\nInteractive: Enter evaluates complete input, supplying one outer ')' when needed; Ctrl+J inserts a newline."
)]
struct Cli {
    /// Load FILE as startup configuration in any mode
    #[arg(long, value_name = "FILE", conflicts_with = "no_config")]
    config: Option<PathBuf>,

    /// Skip automatic configuration and template creation
    #[arg(long)]
    no_config: bool,

    /// Use memory-only history in the interactive shell
    #[arg(long)]
    no_history: bool,

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

fn run(cli: Cli) -> io::Result<i32> {
    let command = cli.command.is_some();
    let mut arguments = cli.command.unwrap_or(cli.script).into_iter();
    let first = arguments.next();
    let interactive = first.is_none() && io::stdin().is_terminal();
    let mut shell = Shell::new(arguments.collect())?;

    if interactive {
        shell.interactive()?;
    }
    if !cli.no_config
        && let Some(code) = startup::load(&mut shell, cli.config.as_deref(), interactive)
    {
        return Ok(code);
    }
    if interactive {
        let history = if cli.no_history {
            None
        } else {
            startup::history_path()
        };
        return editor::run_with_history(&mut shell, history.as_deref());
    }

    match first.as_deref() {
        Some(source) if command => Ok(run_source(&mut shell, "-c", source)),
        Some(file) if file != "-" => {
            let source = fs::read_to_string(file)?;
            Ok(run_source(&mut shell, file, &source))
        }
        _ => {
            let mut source = String::new();
            io::stdin().read_to_string(&mut source)?;
            Ok(run_source(&mut shell, "stdin", &source))
        }
    }
}
