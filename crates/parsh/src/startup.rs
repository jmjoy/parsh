use parsh::shell::Shell;
use std::{
    env, fs,
    io::{self, ErrorKind, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

const TEMPLATE: &[u8] = br#"(; Parsh runs this file before the first interactive prompt. )
(; Remove a surrounding comment to enable its example. )

(; (export EDITOR vi) )
(; (defn hello () (echo "hello from parsh")) )
"#;

/// Return an exit code if configuration loading ends startup.
pub fn load(shell: &mut Shell, config: Option<&Path>, interactive: bool) -> Option<i32> {
    let explicit = config.is_some();
    let path = match config {
        Some(path) => path.to_path_buf(),
        None if interactive => default_config()?,
        None => return None,
    };

    match shell.load_startup_file(&path, interactive) {
        Ok(()) => None,
        Err(error) if error.exit => Some(error.code),
        Err(error) => {
            eprintln!("parsh: {}", error.message);
            explicit.then_some(error.code)
        }
    }
}

fn absolute_environment_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

pub fn history_path() -> Option<PathBuf> {
    let base = absolute_environment_path("XDG_STATE_HOME")
        .or_else(|| absolute_environment_path("HOME").map(|home| home.join(".local/state")))?;
    Some(base.join("parsh/history.jsonl"))
}

fn default_config() -> Option<PathBuf> {
    let base = absolute_environment_path("XDG_CONFIG_HOME")
        .or_else(|| absolute_environment_path("HOME").map(|home| home.join(".config")))?;
    let directory = base.join("parsh");
    let path = directory.join("config.parsh");

    if let Err(error) = initialize_template(&directory, &path) {
        eprintln!(
            "parsh: cannot initialize configuration file {}: {error}",
            path.display()
        );
        return None;
    }
    match fs::symlink_metadata(&path) {
        Ok(_) => Some(path),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            eprintln!("parsh: {}: {error}", path.display());
            None
        }
    }
}

fn initialize_template(directory: &Path, path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.is_symlink() => return fs::metadata(directory).map(|_| ()),
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::create_dir_all(directory.parent().expect("configuration base directory"))?;
    match fs::create_dir(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Ok(()),
        Err(error) => return Err(error),
    }

    let mut template = NamedTempFile::new_in(directory)?;
    template.write_all(TEMPLATE)?;
    match template.persist_noclobber(path) {
        Ok(_) => eprintln!("parsh: created configuration file {}", path.display()),
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.error),
    }
    Ok(())
}
