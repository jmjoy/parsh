//! Session-owned background tasks. Interpreter values cross threads only as owned graphs.
use super::*;
use super::{
    execution::{exited, field, is_record, record, result_record},
    snapshot::{Frozen, TaskSnapshot},
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Condvar,
        atomic::AtomicI64,
        mpsc::{self, RecvTimeoutError, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(crate) struct Control {
    requested: Mutex<Option<Instant>>,
    groups: Mutex<Vec<i32>>,
    startup: Mutex<Option<SyncSender<Result<(), process::ProcessError>>>>,
}
impl Control {
    fn new(startup: Option<SyncSender<Result<(), process::ProcessError>>>) -> Self {
        Self {
            requested: Mutex::new(None),
            groups: Mutex::new(Vec::new()),
            startup: Mutex::new(startup),
        }
    }
    pub fn cancelled(&self) -> bool {
        self.requested.lock().unwrap().is_some()
    }
    pub fn forced(&self) -> bool {
        self.requested
            .lock()
            .unwrap()
            .is_some_and(|t| t.elapsed() >= Duration::from_millis(200))
    }
    pub fn register(&self, group: i32) {
        self.groups.lock().unwrap().push(group);
    }
    pub fn unregister(&self, group: i32) {
        self.groups.lock().unwrap().retain(|g| *g != group);
    }
    pub fn started(&self, result: Result<(), process::ProcessError>) {
        if let Some(sender) = self.startup.lock().unwrap().take() {
            let _ = sender.send(result);
        }
    }
    fn signal_groups(&self, signal: nix::sys::signal::Signal) {
        for group in self.groups.lock().unwrap().iter() {
            let _ = nix::sys::signal::killpg(nix::unistd::Pid::from_raw(*group), signal);
        }
    }
    fn cancel(&self) {
        let mut requested = self.requested.lock().unwrap();
        if requested.is_none() {
            *requested = Some(Instant::now());
        }
        drop(requested);
        self.signal_groups(nix::sys::signal::Signal::SIGTERM);
    }
}

struct Task {
    parent: Option<i64>,
    control: Arc<Control>,
    result: Mutex<Option<Arc<Frozen>>>,
    ready: Condvar,
    join: Mutex<Option<JoinHandle<()>>>,
}
struct State {
    closing: bool,
    next: i64,
    jobs: HashMap<i64, Arc<Task>>,
}
static NEXT_SESSION: AtomicI64 = AtomicI64::new(1);
pub(super) struct Registry {
    session: i64,
    state: Mutex<State>,
}
impl Registry {
    pub fn new() -> Self {
        Self {
            session: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            state: Mutex::new(State {
                closing: false,
                next: 1,
                jobs: HashMap::new(),
            }),
        }
    }
    fn allocate(
        &self,
        expr: &Expr,
        parent: Option<i64>,
        control: Arc<Control>,
    ) -> Result<(i64, Arc<Task>), Error> {
        let mut state = self.state.lock().unwrap();
        if state.closing {
            return Err(Error::new(expr, "the task session is closing"));
        }
        if parent
            .and_then(|id| state.jobs.get(&id))
            .is_some_and(|task| task.control.cancelled())
        {
            return Err(Error {
                code: 130,
                kind: "cancelled".into(),
                cancelled: true,
                ..Error::new(expr, "parent task is cancelled")
            });
        }
        let id = state.next;
        state.next += 1;
        let task = Arc::new(Task {
            parent,
            control,
            result: Mutex::new(None),
            ready: Condvar::new(),
            join: Mutex::new(None),
        });
        state.jobs.insert(id, task.clone());
        Ok((id, task))
    }
    fn task(&self, expr: &Expr, value: &Value) -> Result<(i64, Arc<Task>), Error> {
        let number = |key| match field(value, key) {
            Some(Value::Number(Number::Int(n))) => Some(*n),
            _ => None,
        };
        if !is_record(value, "Job") || number("session") != Some(self.session) {
            return Err(Error::new(expr, "expected a Job from this session"));
        }
        let id = number("id").ok_or_else(|| Error::new(expr, "invalid Job id"))?;
        let task = self
            .state
            .lock()
            .unwrap()
            .jobs
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::new(expr, "unknown Job"))?;
        Ok((id, task))
    }
    fn cancel(&self, id: i64) {
        let state = self.state.lock().unwrap();
        let mut ids = vec![id];
        let mut cursor = 0;
        while cursor < ids.len() {
            let parent = ids[cursor];
            cursor += 1;
            ids.extend(
                state
                    .jobs
                    .iter()
                    .filter(|(_, task)| task.parent == Some(parent))
                    .map(|(id, _)| *id),
            );
        }
        for id in ids {
            if let Some(task) = state.jobs.get(&id)
                && task.result.lock().unwrap().is_none()
            {
                task.control.cancel();
            }
        }
    }
    pub fn shutdown(&self) {
        let tasks: Vec<_> = {
            let mut state = self.state.lock().unwrap();
            state.closing = true;
            state.jobs.values().cloned().collect()
        };
        for task in &tasks {
            if task.result.lock().unwrap().is_none() {
                task.control.cancel();
            }
        }
        for task in &tasks {
            let mut result = task.result.lock().unwrap();
            while result.is_none() {
                if task.control.forced() {
                    task.control
                        .signal_groups(nix::sys::signal::Signal::SIGKILL);
                }
                result = task
                    .ready
                    .wait_timeout(result, Duration::from_millis(10))
                    .unwrap()
                    .0;
            }
            drop(result);
            if let Some(handle) = task.join.lock().unwrap().take() {
                let _ = handle.join();
            }
        }
        self.state.lock().unwrap().jobs.clear();
    }
}

// Native file handles are Send; the general Io also holds non-Send browser handles.
struct TaskIo {
    input: Arc<File>,
    output: Option<Arc<File>>,
    error: Option<Arc<File>>,
}
impl TaskIo {
    fn new(expr: &Expr, io: &Io) -> Result<Self, Error> {
        Ok(Self {
            input: match &io.input {
                Some(file) => file.clone(),
                None => {
                    Arc::new(File::open("/dev/null").map_err(|e| Error::new(expr, e.to_string()))?)
                }
            },
            output: match &io.output {
                Output::File(file) => Some(file.clone()),
                _ => None,
            },
            error: io.error.clone(),
        })
    }
    fn streams(self) -> Io {
        Io {
            input: Some(self.input),
            output: self.output.map_or(Output::Inherit, Output::File),
            error: self.error,
            ..Io::default()
        }
    }
}

impl Shell {
    fn restore_task_result(&mut self, id: i64, frozen: &Frozen) -> Value {
        if let Some(result) = self.task_cache.get(&id) {
            return result.clone();
        }
        let result = frozen.restore(self);
        self.task_cache.insert(id, result.clone());
        result
    }

    fn wait_task(&mut self, expr: &Expr, id: i64, task: &Task) -> Result<Value, Error> {
        if self.task_id == Some(id) {
            return Err(Error::new(expr, "a task cannot wait for itself"));
        }
        let frozen = {
            let mut result = task.result.lock().unwrap();
            loop {
                if let Some(result) = &*result {
                    break result.clone();
                }
                self.check(expr)?;
                result = task
                    .ready
                    .wait_timeout(result, Duration::from_millis(10))
                    .unwrap()
                    .0;
            }
        };
        if let Some(handle) = task.join.lock().unwrap().take() {
            let _ = handle.join();
        }
        Ok(self.restore_task_result(id, &frozen))
    }

    fn spawn_task(&mut self, expr: &Expr, mut values: Vec<Value>, io: &Io) -> EvalResult<Value> {
        let callee = self.resolve_callable(expr, values.remove(0))?;
        let direct = matches!(callee, Value::Command(_));
        if let Value::Command(name) = &callee {
            Self::argv_values(expr, name, values.clone())?;
        }
        let snapshot = TaskSnapshot::new(self, &callee, &values);
        let streams = TaskIo::new(expr, io)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let control = Arc::new(Control::new(direct.then_some(sender)));
        let (id, task) = self.tasks.allocate(expr, self.task_id, control.clone())?;
        let registry = self.tasks.clone();
        let running = task.clone();
        let source = expr.clone();
        let handle = thread::Builder::new()
            .name(format!("parsh-task-{id}"))
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                let transferred = catch_unwind(AssertUnwindSafe(|| {
                    let mut shell = Shell::syntax();
                    shell.task_id = Some(id);
                    shell.tasks = registry;
                    shell.task_control = Some(control.clone());
                    let (callee, values) = snapshot.restore(&mut shell);
                    let env = shell.root.clone();
                    let streams = streams.streams();
                    let result =
                        match shell.capture_call(&source, callee, &values, &env, &streams, true) {
                            Ok(result) => result,
                            Err(signal) => {
                                let error = signal.into_error(&source);
                                result_record(
                                    Value::Nil,
                                    if error.exit {
                                        exited(error.code)
                                    } else {
                                        Value::Nil
                                    },
                                    if error.exit {
                                        Value::Nil
                                    } else {
                                        error.as_value(false)
                                    },
                                    Value::List(Vec::new()),
                                )
                            }
                        };
                    // Startup failures before process::run must also release spawn's handshake.
                    let startup = match field(&result, "error") {
                        Some(error) if !matches!(error, Value::Nil) => Err(process::ProcessError {
                            code: match field(error, "code") {
                                Some(Value::Number(Number::Int(n))) => *n as i32,
                                _ => 2,
                            },
                            message: match field(error, "message") {
                                Some(Value::Text(s)) => s.clone(),
                                _ => "task startup failed".into(),
                            },
                        }),
                        _ => Ok(()),
                    };
                    control.started(startup);
                    Frozen::new(&result)
                }));
                let frozen = transferred.unwrap_or_else(|_| {
                    let error = Error {
                        kind: "task-panic".into(),
                        ..Error::new(&source, "background interpreter panicked")
                    };
                    control.started(Err(process::ProcessError {
                        code: 2,
                        message: error.message.clone(),
                    }));
                    Frozen::new(&result_record(
                        Value::Nil,
                        Value::Nil,
                        error.as_value(false),
                        Value::List(Vec::new()),
                    ))
                });
                *running.result.lock().unwrap() = Some(Arc::new(frozen));
                running.ready.notify_all();
            })
            .map_err(|e| Error::new(expr, format!("could not start task: {e}")));
        match handle {
            Ok(handle) => *task.join.lock().unwrap() = Some(handle),
            Err(error) => {
                self.tasks.state.lock().unwrap().jobs.remove(&id);
                return Err(error.into());
            }
        }
        if direct {
            let started = loop {
                match receiver.recv_timeout(Duration::from_millis(10)) {
                    Ok(started) => break started,
                    Err(RecvTimeoutError::Timeout) => {
                        if let Err(error) = self.check(expr) {
                            self.tasks.cancel(id);
                            return Err(error.into());
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        break Err(process::ProcessError {
                            code: 2,
                            message: "task did not report startup".into(),
                        });
                    }
                }
            };
            if let Err(error) = started {
                let result = self.wait_task(expr, id, &task)?;
                self.publish(result.clone());
                return Err(Error {
                    code: error.code,
                    kind: "spawn-failed".into(),
                    result: Some(Box::new(result)),
                    ..Error::new(expr, error.message)
                }
                .into());
            }
        }
        let job = record(
            "Job",
            [
                ("session", Value::Number(Number::Int(self.tasks.session))),
                ("id", Value::Number(Number::Int(id))),
            ],
        );
        self.publish(result_record(
            job.clone(),
            Value::Nil,
            Value::Nil,
            Value::List(Vec::new()),
        ));
        Ok(job)
    }

    pub(super) fn task_builtin(
        &mut self,
        expr: &Expr,
        name: &str,
        values: Vec<Value>,
        _env: &Env,
        io: &Io,
    ) -> EvalResult<Value> {
        if name == "spawn" {
            return self.spawn_task(expr, values, io);
        }
        let (id, task) = self.tasks.task(expr, &values[0])?;
        match name {
            "wait" => {
                let result = self.wait_task(expr, id, &task)?;
                self.publish(result.clone());
                Ok(result)
            }
            "cancel" => {
                self.tasks.cancel(id);
                Ok(Value::Nil)
            }
            "job-status" => {
                let frozen = task.result.lock().unwrap().clone();
                let result = frozen
                    .as_ref()
                    .map_or(Value::Nil, |r| self.restore_task_result(id, r));
                Ok(record(
                    "JobStatus",
                    [
                        (
                            "state",
                            Value::Text(
                                if frozen.is_some() {
                                    "completed"
                                } else {
                                    "running"
                                }
                                .into(),
                            ),
                        ),
                        ("result", result),
                    ],
                ))
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::{Read, Reader};

    fn expression(source: &str) -> Expr {
        let Read::Form(expr) = Reader::new(source).read() else {
            panic!("invalid test source")
        };
        expr
    }

    #[test]
    fn resetting_native_sessions_reaps_tasks_invalidates_handles_and_keeps_native_capabilities() {
        let mut shell = Shell::new(vec!["argument".into()]).unwrap();
        let old = shell.execute(&expression("(spawn sleep 30)")).unwrap();
        shell.reset_session().unwrap();
        assert!(matches!(shell.last_result, Value::Nil));
        assert_eq!(
            shell.root.borrow().variables["args"],
            Value::List(vec![Value::Text("argument".into())])
        );
        let env = shell.root.clone();
        let error = shell
            .task_builtin(
                &expression("(wait old)"),
                "wait",
                vec![old],
                &env,
                &Io::default(),
            )
            .unwrap_err();
        assert!(
            error
                .into_error(&expression("(wait old)"))
                .message
                .contains("from this session")
        );
        let next = shell.execute(&expression("(spawn + #1 #2)")).unwrap();
        let result = shell
            .task_builtin(
                &expression("(wait next)"),
                "wait",
                vec![next],
                &env,
                &Io::default(),
            )
            .unwrap();
        assert_eq!(
            field(&result, "value"),
            Some(&Value::Number(Number::Int(3)))
        );
    }
}
