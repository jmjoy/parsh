#[cfg(feature = "native")]
use std::{
    fs::File,
    sync::{Arc, Mutex},
};

#[cfg(feature = "native")]
#[derive(Clone, Default)]
pub enum Output {
    #[default]
    Inherit,
    File(Arc<File>),
    Capture(Arc<Mutex<Vec<u8>>>),
}

#[cfg(feature = "native")]
#[derive(Clone, Default)]
pub struct Io {
    pub input: Option<Arc<File>>,
    pub output: Output,
    pub error: Option<Arc<File>>,
}

#[cfg(not(feature = "native"))]
#[derive(Clone, Default)]
pub(crate) struct Io {}
