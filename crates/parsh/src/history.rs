use std::collections::VecDeque;

#[cfg(feature = "native")]
pub(crate) mod disk;

const MAX_ENTRIES: usize = 10_000;
const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct Entries {
    pub(crate) sources: VecDeque<String>,
    bytes: usize,
}

impl Entries {
    pub(crate) fn push(&mut self, source: &str) {
        if source.len() > MAX_SOURCE_BYTES
            || source.trim().is_empty()
            || self.sources.back().is_some_and(|last| last == source)
        {
            return;
        }
        while self.sources.len() >= MAX_ENTRIES || self.bytes + source.len() > MAX_SOURCE_BYTES {
            self.bytes -= self.sources.pop_front().expect("history to trim").len();
        }
        self.bytes += source.len();
        self.sources.push_back(source.to_owned());
    }
}
