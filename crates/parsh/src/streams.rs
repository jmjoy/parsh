use crate::host::ExecutionHost;
#[cfg(feature = "native")]
use std::fs::File;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

pub(crate) type SharedHost = Rc<RefCell<dyn ExecutionHost>>;

pub(crate) struct HostFile {
    pub handle: u32,
    pub host: SharedHost,
}

impl Drop for HostFile {
    fn drop(&mut self) {
        self.host.borrow_mut().close(self.handle);
    }
}

#[derive(Clone, Default)]
pub enum Output {
    #[default]
    Inherit,
    #[cfg(feature = "native")]
    File(Arc<File>),
    Capture(Arc<Mutex<Vec<u8>>>),
}

#[derive(Clone, Default)]
pub struct Io {
    #[cfg(feature = "native")]
    pub input: Option<Arc<File>>,
    pub output: Output,
    pub error_capture: Option<Arc<Mutex<Vec<u8>>>>,
    pub capture_budget: Option<Arc<CaptureBudget>>,
    #[cfg(feature = "native")]
    pub error: Option<Arc<File>>,
    pub(crate) host_input: Option<Rc<HostFile>>,
    pub(crate) host_output: Option<Rc<HostFile>>,
    pub(crate) host_error: Option<Rc<HostFile>>,
}

/// One budget shared by both streams of a capture or task.
#[derive(Default)]
pub struct CaptureBudget {
    used: AtomicUsize,
    pub exceeded: AtomicBool,
}

pub(crate) fn append_capture(
    buffer: &Arc<Mutex<Vec<u8>>>,
    budget: Option<&Arc<CaptureBudget>>,
    bytes: &[u8],
) -> std::io::Result<()> {
    let mut take = bytes.len();
    if let Some(budget) = budget {
        let previous = budget.used.fetch_add(bytes.len(), Ordering::Relaxed);
        take = take.min(crate::host::MAX_OUTPUT_BYTES.saturating_sub(previous));
        if take < bytes.len() {
            budget.exceeded.store(true, Ordering::Relaxed);
        }
    }
    buffer.lock().unwrap().extend_from_slice(&bytes[..take]);
    if take < bytes.len() {
        Err(std::io::Error::other("command output exceeds 8 MiB"))
    } else {
        Ok(())
    }
}
