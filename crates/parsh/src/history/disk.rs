use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use super::Entries;
use crate::reader::{Kind, Read as FormRead, Reader};
use tempfile::NamedTempFile;

const COMPACT_AT: u64 = 16 * 1024 * 1024;
const MAX_ENCODED_BYTES: usize = 8 * 1024 * 1024;

pub(crate) struct HistoryFile {
    path: PathBuf,
    lock_path: PathBuf,
}

impl HistoryFile {
    pub(crate) fn open(path: &Path) -> io::Result<(Self, Entries)> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let path = parent.canonicalize()?.join(path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "missing history filename")
        })?);
        let store = Self {
            lock_path: path.with_added_extension("lock"),
            path,
        };
        let lock = store.lock_file()?;
        lock.lock_shared()?;
        let entries = match open_regular(&store.path, false) {
            Ok(mut file) => read_entries(&mut file)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Entries::default(),
            Err(error) => return Err(error),
        };
        Ok((store, entries))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    fn lock_file(&self) -> io::Result<File> {
        // Lock a separate inode so replacing the history file never splits the writers.
        open_regular(&self.lock_path, true)
    }

    pub(crate) fn append(&self, source: &str) -> io::Result<()> {
        if source.len() >= MAX_ENCODED_BYTES {
            return Ok(());
        }
        let line = encode(source)?;
        if line.len() > MAX_ENCODED_BYTES {
            return Ok(());
        }

        let lock = self.lock_file()?;
        lock.lock()?;
        let mut file = open_regular(&self.path, true)?;
        let length = file.metadata()?.len();
        if length.saturating_add(line.len() as u64) >= COMPACT_AT {
            let mut entries = read_entries(&mut file)?;
            entries.push(source);
            return self.compact(&entries);
        }
        let end = remove_partial_line(&mut file, length)?;
        if !ends_with_record(&mut file, end, &line)? {
            file.seek(SeekFrom::Start(end))?;
            file.write_all(&line)?;
        }
        Ok(())
    }

    fn compact(&self, entries: &Entries) -> io::Result<()> {
        let directory = self.path.parent().expect("absolute history path");
        let mut file = NamedTempFile::new_in(directory)?;
        let mut lines = Vec::new();
        let mut bytes = 0;
        for source in entries.sources.iter().rev() {
            let line = encode(source)?;
            bytes += line.len();
            if bytes > MAX_ENCODED_BYTES {
                break;
            }
            lines.push(line);
        }
        for line in lines.iter().rev() {
            file.write_all(line)?;
        }
        file.persist(&self.path).map_err(|error| error.error)?;
        Ok(())
    }
}

fn open_regular(path: &Path, writable: bool) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(writable)
        .create(writable)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a regular history file",
        ));
    }
    Ok(file)
}

fn encode(source: &str) -> io::Result<Vec<u8>> {
    let mut line = serde_json::to_vec(source)?;
    line.push(b'\n');
    Ok(line)
}

fn read_entries(file: &mut File) -> io::Result<Entries> {
    file.rewind()?;
    let mut reader = BufReader::new(file);
    let mut entries = Entries::default();
    let mut line = Vec::new();
    loop {
        line.clear();
        let length = (&mut reader)
            .take(MAX_ENCODED_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if length == 0 {
            return Ok(entries);
        }
        if !line.ends_with(b"\n") {
            // A too-long line or an interrupted write cannot be a complete record.
            reader.skip_until(b'\n')?;
        } else if length <= MAX_ENCODED_BYTES
            && let Ok(source) = serde_json::from_slice::<String>(&line)
            && is_history_source(&source)
        {
            entries.push(&source);
        }
    }
}

fn is_history_source(source: &str) -> bool {
    let mut reader = Reader::new(source);
    matches!(reader.read(), FormRead::Form(expr) if expr.is_top_level() && expr.kind != Kind::Comment)
        && matches!(reader.read(), FormRead::End)
}

fn remove_partial_line(file: &mut File, length: u64) -> io::Result<u64> {
    if length == 0 {
        return Ok(0);
    }
    file.seek(SeekFrom::Start(length - 1))?;
    let mut last = [0];
    file.read_exact(&mut last)?;
    if last[0] == b'\n' {
        return Ok(length);
    }

    let mut end = length - 1;
    let mut block = [0; 8192];
    while end > 0 {
        let start = end.saturating_sub(block.len() as u64);
        let chunk = &mut block[..(end - start) as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(chunk)?;
        if let Some(index) = chunk.iter().rposition(|&byte| byte == b'\n') {
            end = start + index as u64 + 1;
            break;
        }
        end = start;
    }
    file.set_len(end)?;
    Ok(end)
}

fn ends_with_record(file: &mut File, end: u64, line: &[u8]) -> io::Result<bool> {
    let Some(start) = end.checked_sub(line.len() as u64) else {
        return Ok(false);
    };
    file.seek(SeekFrom::Start(start.saturating_sub(1)))?;
    if start > 0 {
        let mut separator = [0];
        file.read_exact(&mut separator)?;
        if separator[0] != b'\n' {
            return Ok(false);
        }
    }
    let mut previous = vec![0; line.len()];
    file.read_exact(&mut previous)?;
    Ok(previous == line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::MAX_ENTRIES;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn loads_complete_forms_skips_invalid_records_and_repairs_the_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let (store, empty) = HistoryFile::open(&path).unwrap();
        assert!(empty.sources.is_empty());
        let first = "(str\n  '甲')";
        store.append(first).unwrap();
        store.append(first).unwrap();
        let good_length = encode(first).unwrap().len();
        assert_eq!(fs::metadata(&path).unwrap().len(), good_length as u64);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"bad json\n").unwrap();
        file.write_all(&encode("(str").unwrap()).unwrap();
        file.write_all(&encode("(; comment )").unwrap()).unwrap();
        file.write_all(&vec![b'x'; MAX_ENCODED_BYTES + 1]).unwrap();
        file.write_all(b"\n").unwrap();
        let second = "(str complete)";
        file.write_all(&encode(second).unwrap()).unwrap();
        file.write_all(b"\"(str broken").unwrap();
        drop(file);
        let (_, entries) = HistoryFile::open(&path).unwrap();
        assert_eq!(entries.sources, [first, second]);

        let last = "(str recovered)";
        store.append(last).unwrap();
        let (_, entries) = HistoryFile::open(&path).unwrap();
        assert_eq!(entries.sources, [first, second, last]);
        let contents = fs::read(&path).unwrap();
        assert!(contents.ends_with(&encode(last).unwrap()));
        assert!(!contents.windows(6).any(|bytes| bytes == b"broken"));
        for file in [&path, &store.lock_path] {
            assert_eq!(
                fs::metadata(file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn compaction_keeps_the_newest_records_within_both_limits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let (store, _) = HistoryFile::open(&path).unwrap();
        let mut file = File::create(&path).unwrap();
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .unwrap();
        file.set_len(COMPACT_AT).unwrap();
        file.seek(SeekFrom::End(0)).unwrap();
        file.write_all(b"\n").unwrap();
        for entry in 0..MAX_ENTRIES + 1 {
            file.write_all(&encode(&format!("(str entry-{entry})")).unwrap())
                .unwrap();
        }
        let multiline = format!("(str '''{}''')", "\t".repeat(3 * 1024 * 1024));
        file.write_all(&encode(&multiline).unwrap()).unwrap();
        drop(file);
        store.append("(str newest)").unwrap();
        assert!(fs::metadata(&path).unwrap().len() <= MAX_ENCODED_BYTES as u64);
        let (_, entries) = HistoryFile::open(&path).unwrap();
        assert_eq!(entries.sources.len(), MAX_ENTRIES);
        assert_eq!(entries.sources[0], "(str entry-3)");
        assert_eq!(entries.sources[MAX_ENTRIES - 2], multiline);
        assert_eq!(entries.sources[MAX_ENTRIES - 1], "(str newest)");

        let another = format!("(str '''{}''')", "\t".repeat(2 * 1024 * 1024));
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(&vec![b'x'; COMPACT_AT as usize]).unwrap();
        file.write_all(b"\n").unwrap();
        drop(file);
        store.append(&another).unwrap();
        let (_, entries) = HistoryFile::open(&path).unwrap();
        assert_eq!(entries.sources, [String::from("(str newest)"), another]);
        assert!(fs::metadata(&path).unwrap().len() <= MAX_ENCODED_BYTES as u64);
    }
}
