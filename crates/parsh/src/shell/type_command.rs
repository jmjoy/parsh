use super::*;

#[cfg(feature = "native")]
use std::os::unix::ffi::OsStrExt;

#[derive(Clone, Copy, Default, PartialEq)]
enum Format {
    #[default]
    Description,
    Kind,
    Path,
}

#[derive(Default)]
struct Options {
    all: bool,
    skip_functions: bool,
    force_path: bool,
    format: Format,
}

impl Options {
    fn parse<'a>(expr: &Expr, args: &'a [String]) -> Result<(Self, &'a [String]), Error> {
        let mut options = Self::default();
        let mut names = args;
        while let Some(arg) = names.first() {
            if arg == "--" {
                names = &names[1..];
                break;
            }
            if !arg.starts_with('-') || arg == "-" {
                break;
            }
            for flag in arg[1..].chars() {
                match flag {
                    'a' => options.all = true,
                    'f' => options.skip_functions = true,
                    't' => options.format = Format::Kind,
                    'p' => options.format = Format::Path,
                    'P' => {
                        options.force_path = true;
                        options.format = Format::Path;
                    }
                    _ => {
                        return Err(Error::new(
                            expr,
                            format!(
                                "type: -{flag}: invalid option; usage: type [-afptP] [name ...]"
                            ),
                        ));
                    }
                }
            }
            names = &names[1..];
        }
        Ok((options, names))
    }
}

enum CommandType {
    Builtin,
    Function,
    #[cfg(feature = "native")]
    File(Vec<u8>),
    Host,
}

impl CommandType {
    fn output(&self, name: &str, format: Format) -> Vec<u8> {
        let text = match (format, self) {
            (Format::Kind, Self::Builtin) => "builtin\n".into(),
            (Format::Kind, Self::Function) => "function\n".into(),
            (Format::Kind, Self::Host) => "file\n".into(),
            #[cfg(feature = "native")]
            (Format::Kind, Self::File(_)) => "file\n".into(),
            (Format::Path, Self::Builtin | Self::Function) => return Vec::new(),
            (Format::Path, Self::Host) => format!("{name}\n"),
            (Format::Description, Self::Builtin) => format!("{name} is a shell builtin\n"),
            (Format::Description, Self::Function) => format!("{name} is a function\n"),
            (Format::Description, Self::Host) => format!("{name} is an external command\n"),
            #[cfg(feature = "native")]
            (Format::Description | Format::Path, Self::File(path)) => {
                let mut output = if format == Format::Description {
                    format!("{name} is ").into_bytes()
                } else {
                    Vec::new()
                };
                output.extend(path);
                output.push(b'\n');
                return output;
            }
        };
        text.into_bytes()
    }
}

impl Shell {
    pub(super) fn type_command(
        &mut self,
        expr: &Expr,
        args: &[Expr],
        env: &Env,
        mode: Mode,
        io: &Io,
    ) -> EvalResult<Value> {
        let argv = self.argv("type", args, env, io)?;
        let (options, names) = Options::parse(expr, &argv[1..])?;
        let mut code = 0;
        for name in names {
            self.check(expr)?;
            let mut matches = Vec::new();
            if !options.force_path {
                if builtin(name) {
                    matches.push(CommandType::Builtin);
                } else if !options.skip_functions && self.functions.contains_key(name) {
                    matches.push(CommandType::Function);
                }
            }
            if options.all || matches.is_empty() {
                matches.extend(self.external_types(expr, name, options.all)?);
            }
            if matches.is_empty() {
                code = 1;
                if options.format == Format::Description {
                    self.write_output(
                        expr,
                        io,
                        Stream::Stderr,
                        format!("type: {name}: not found\n").into_bytes(),
                    )?;
                }
            } else {
                for command in matches {
                    let bytes = command.output(name, options.format);
                    if !bytes.is_empty() {
                        self.write_output(expr, io, Stream::Stdout, bytes)?;
                    }
                }
            }
        }
        Ok(Self::command_status(expr, "type", code, mode)?)
    }

    fn external_types(
        &mut self,
        expr: &Expr,
        name: &str,
        all: bool,
    ) -> Result<Vec<CommandType>, Error> {
        if self.syntax_only || name.is_empty() || name.contains('\0') {
            return Ok(Vec::new());
        }
        if let Some(host) = &self.host {
            return Ok(
                if host
                    .borrow_mut()
                    .commands()
                    .map_err(|error| Self::host_error(expr, error))?
                    .iter()
                    .any(|command| command == name)
                {
                    vec![CommandType::Host]
                } else {
                    Vec::new()
                },
            );
        }
        #[cfg(feature = "native")]
        {
            if name.contains('/') {
                return Ok(if executable_file(&self.path(name)) {
                    vec![CommandType::File(name.as_bytes().into())]
                } else {
                    Vec::new()
                });
            }
            let mut matches = Vec::new();
            for directory in std::env::split_paths(&self.search_path()) {
                self.check(expr)?;
                let path = if directory.as_os_str().is_empty() {
                    Path::new(".").join(name)
                } else {
                    directory.join(name)
                };
                if executable_file(&self.cwd.join(&path)) {
                    matches.push(CommandType::File(path.as_os_str().as_bytes().into()));
                    if !all {
                        break;
                    }
                }
            }
            Ok(matches)
        }
        #[cfg(not(feature = "native"))]
        {
            let _ = all;
            Ok(Vec::new())
        }
    }
}
