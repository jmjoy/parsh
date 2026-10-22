use crate::host::ExecutionHost;
#[cfg(feature = "native")]
use std::fs::File;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
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
    #[cfg(feature = "native")]
    pub error: Option<Arc<File>>,
    pub(crate) host_input: Option<Rc<HostFile>>,
    pub(crate) host_output: Option<Rc<HostFile>>,
    pub(crate) host_error: Option<Rc<HostFile>>,
}
