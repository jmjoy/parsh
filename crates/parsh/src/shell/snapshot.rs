//! Owned graph transfer. No Rc, RefCell, or live interpreter crosses a thread boundary.
use super::functions::{Callable, PartialCall};
use super::*;

#[derive(Clone)]
enum FrozenValue {
    Text(String),
    Number(Number),
    Bool(bool),
    List(Vec<FrozenValue>),
    Map(IndexMap<String, FrozenValue>),
    Form(Expr),
    Function(usize),
    Command(String),
    Bytes(Arc<[u8]>),
    Struct(String, IndexMap<String, FrozenValue>),
    Nil,
}
#[derive(Clone)]
struct FrozenScope {
    variables: HashMap<String, usize>,
    parent: Option<usize>,
    fallback: Option<usize>,
}
#[derive(Clone)]
struct Definition {
    parameters: Parameters,
    body: Vec<Expr>,
    closure: usize,
}
#[derive(Clone)]
enum FrozenFunction {
    User(String, Definition),
    Builtin(String),
    Partial {
        name: String,
        closure: usize,
        preset_markers: Option<Vec<(usize, String)>>,
    },
    ExternalAlias(String, Vec<String>),
}

#[derive(Default)]
struct Builder {
    scope_ids: HashMap<usize, usize>,
    binding_ids: HashMap<usize, usize>,
    function_ids: HashMap<usize, usize>,
    scopes: Vec<Option<FrozenScope>>,
    bindings: Vec<Option<FrozenValue>>,
    functions: Vec<Option<FrozenFunction>>,
}
impl Builder {
    fn scope(&mut self, env: &Env) -> usize {
        let key = Rc::as_ptr(env) as usize;
        if let Some(id) = self.scope_ids.get(&key) {
            return *id;
        }
        let id = self.scopes.len();
        self.scope_ids.insert(key, id);
        self.scopes.push(None);
        let (parent_env, fallback_env, variables) = {
            let scope = env.borrow();
            (
                scope.parent.clone(),
                scope.fallback.clone(),
                scope
                    .variables
                    .iter()
                    .map(|(name, binding)| (name.clone(), binding.clone()))
                    .collect::<Vec<_>>(),
            )
        };
        let parent = parent_env.as_ref().map(|parent| self.scope(parent));
        let fallback = fallback_env.as_ref().map(|fallback| self.scope(fallback));
        let variables = variables
            .into_iter()
            .map(|(name, binding)| (name, self.binding(&binding)))
            .collect();
        self.scopes[id] = Some(FrozenScope {
            variables,
            parent,
            fallback,
        });
        id
    }

    fn binding(&mut self, binding: &Binding) -> usize {
        let key = Rc::as_ptr(binding) as usize;
        if let Some(id) = self.binding_ids.get(&key) {
            return *id;
        }
        let id = self.bindings.len();
        self.binding_ids.insert(key, id);
        self.bindings.push(None);
        let value = binding.borrow().clone();
        self.bindings[id] = Some(self.value(&value));
        id
    }
    fn definition(&mut self, f: &Function) -> Definition {
        Definition {
            parameters: f.parameters.clone(),
            body: f.body.clone(),
            closure: self.scope(&f.closure),
        }
    }
    fn function(&mut self, f: &FunctionValue) -> usize {
        let key = Rc::as_ptr(&f.0) as usize;
        if let Some(id) = self.function_ids.get(&key) {
            return *id;
        }
        let id = self.functions.len();
        self.function_ids.insert(key, id);
        self.functions.push(None);
        let value = match &*f.0 {
            Callable::User { name, definition } => {
                FrozenFunction::User(name.clone(), self.definition(definition))
            }
            Callable::Builtin(name) => FrozenFunction::Builtin(name.clone()),
            Callable::Partial(PartialCall {
                name,
                closure,
                preset_markers,
            }) => FrozenFunction::Partial {
                name: name.clone(),
                closure: self.scope(closure),
                preset_markers: preset_markers.clone(),
            },
            Callable::ExternalAlias { name, prefix } => {
                FrozenFunction::ExternalAlias(name.clone(), prefix.clone())
            }
        };
        self.functions[id] = Some(value);
        id
    }
    fn value(&mut self, value: &Value) -> FrozenValue {
        match value {
            Value::Text(s) => FrozenValue::Text(s.clone()),
            Value::Number(n) => FrozenValue::Number(*n),
            Value::Bool(b) => FrozenValue::Bool(*b),
            Value::List(v) => FrozenValue::List(v.iter().map(|v| self.value(v)).collect()),
            Value::Map(v) => {
                FrozenValue::Map(v.iter().map(|(k, v)| (k.clone(), self.value(v))).collect())
            }
            Value::Form(f) => FrozenValue::Form((**f).clone()),
            Value::Function(f) => FrozenValue::Function(self.function(f)),
            Value::Command(s) => FrozenValue::Command(s.clone()),
            Value::Bytes(b) => FrozenValue::Bytes(b.clone()),
            Value::Struct { name, fields } => FrozenValue::Struct(
                name.clone(),
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), self.value(v)))
                    .collect(),
            ),
            Value::Nil => FrozenValue::Nil,
        }
    }
    fn finish(self, value: FrozenValue) -> Frozen {
        Frozen {
            value,
            scopes: self.scopes.into_iter().map(Option::unwrap).collect(),
            bindings: self.bindings.into_iter().map(Option::unwrap).collect(),
            functions: self.functions.into_iter().map(Option::unwrap).collect(),
        }
    }
}

#[derive(Clone)]
pub(super) struct Frozen {
    value: FrozenValue,
    scopes: Vec<FrozenScope>,
    bindings: Vec<FrozenValue>,
    functions: Vec<FrozenFunction>,
}
struct Restored {
    scopes: Vec<Env>,
    bindings: Vec<Binding>,
    functions: Vec<FunctionValue>,
}
impl Restored {
    fn definition(&self, f: &Definition) -> Function {
        Function {
            parameters: f.parameters.clone(),
            body: f.body.clone(),
            closure: self.scopes[f.closure].clone(),
        }
    }
    fn value(&self, value: &FrozenValue) -> Value {
        match value {
            FrozenValue::Text(s) => Value::Text(s.clone()),
            FrozenValue::Number(n) => Value::Number(*n),
            FrozenValue::Bool(b) => Value::Bool(*b),
            FrozenValue::List(v) => Value::List(v.iter().map(|v| self.value(v)).collect()),
            FrozenValue::Map(v) => {
                Value::Map(v.iter().map(|(k, v)| (k.clone(), self.value(v))).collect())
            }
            FrozenValue::Form(f) => Value::Form(Box::new(f.clone())),
            FrozenValue::Function(id) => Value::Function(self.functions[*id].clone()),
            FrozenValue::Command(s) => Value::Command(s.clone()),
            FrozenValue::Bytes(b) => Value::Bytes(b.clone()),
            FrozenValue::Struct(name, fields) => Value::Struct {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.clone(), self.value(v)))
                    .collect(),
            },
            FrozenValue::Nil => Value::Nil,
        }
    }
}
impl Frozen {
    pub fn new(value: &Value) -> Self {
        let mut builder = Builder::default();
        let value = builder.value(value);
        builder.finish(value)
    }
    fn nodes(&self, shell: &mut Shell) -> Restored {
        let mut restored = Restored {
            scopes: self
                .scopes
                .iter()
                .map(|_| Rc::new(RefCell::new(Scope::default())))
                .collect(),
            bindings: self
                .bindings
                .iter()
                .map(|_| Rc::new(RefCell::new(Value::Nil)))
                .collect(),
            functions: Vec::new(),
        };
        // Scope placeholders allow functions to point to themselves through bindings.
        restored.functions = self
            .functions
            .iter()
            .map(|f| match f {
                FrozenFunction::User(name, definition) => {
                    FunctionValue::user(name, restored.definition(definition))
                }
                FrozenFunction::Builtin(name) => {
                    FunctionValue(Rc::new(Callable::Builtin(name.clone())))
                }
                FrozenFunction::Partial {
                    name,
                    closure,
                    preset_markers,
                } => FunctionValue(Rc::new(Callable::Partial(PartialCall {
                    name: name.clone(),
                    closure: restored.scopes[*closure].clone(),
                    preset_markers: preset_markers.clone(),
                }))),
                FrozenFunction::ExternalAlias(name, prefix) => {
                    FunctionValue::external_alias(name, prefix.clone())
                }
            })
            .collect();
        for (id, scope) in self.scopes.iter().enumerate() {
            let mut target = restored.scopes[id].borrow_mut();
            target.parent = scope.parent.map(|p| restored.scopes[p].clone());
            target.fallback = scope.fallback.map(|p| restored.scopes[p].clone());
            target.variables = scope
                .variables
                .iter()
                .map(|(k, id)| (k.clone(), restored.bindings[*id].clone()))
                .collect();
        }
        for (id, value) in self.bindings.iter().enumerate() {
            *restored.bindings[id].borrow_mut() = restored.value(value);
        }
        shell
            .scopes
            .extend(restored.scopes.iter().map(Rc::downgrade));
        restored
    }
    pub fn restore(&self, shell: &mut Shell) -> Value {
        self.nodes(shell).value(&self.value)
    }
}

pub(super) struct TaskSnapshot {
    graph: Frozen,
    root: usize,
    named: HashMap<String, usize>,
    aliases: HashMap<String, (usize, Expr)>,
    macros: HashMap<String, Definition>,
    structs: HashMap<String, StructDef>,
    environment: HashMap<OsString, OsString>,
    cwd: PathBuf,
    last_result: FrozenValue,
    load_paths: Vec<OsString>,
}
impl TaskSnapshot {
    pub fn new(shell: &Shell, callee: &Value, values: &[Value]) -> Self {
        let mut builder = Builder::default();
        let root = builder.scope(&shell.root);
        let named = shell
            .functions
            .iter()
            .map(|(k, v)| (k.clone(), builder.function(v)))
            .collect();
        let aliases = shell
            .aliases
            .iter()
            .map(|(name, alias)| {
                (
                    name.clone(),
                    (builder.function(&alias.function), alias.definition.clone()),
                )
            })
            .collect();
        let macros = shell
            .macros
            .iter()
            .map(|(k, v)| (k.clone(), builder.definition(v)))
            .collect();
        let last_result = builder.value(&shell.last_result);
        let bundle = builder.value(&Value::List(vec![
            callee.clone(),
            Value::List(values.to_vec()),
        ]));
        Self {
            graph: builder.finish(bundle),
            root,
            named,
            aliases,
            macros,
            structs: shell.structs.clone(),
            environment: shell.environment.clone(),
            cwd: shell.cwd.clone(),
            last_result,
            load_paths: shell.load_paths.clone(),
        }
    }
    pub fn restore(self, shell: &mut Shell) -> (Value, Vec<Value>) {
        let nodes = self.graph.nodes(shell);
        shell.root = nodes.scopes[self.root].clone();
        shell.functions = self
            .named
            .into_iter()
            .map(|(k, id)| (k, nodes.functions[id].clone()))
            .collect();
        shell.aliases = self
            .aliases
            .into_iter()
            .map(|(name, (id, definition))| {
                (
                    name,
                    aliases::Alias {
                        function: nodes.functions[id].clone(),
                        definition,
                    },
                )
            })
            .collect();
        shell.macros = self
            .macros
            .into_iter()
            .map(|(k, v)| (k, nodes.definition(&v)))
            .collect();
        shell.structs = self.structs;
        shell.environment = self.environment;
        shell.cwd = self.cwd;
        shell.last_result = nodes.value(&self.last_result);
        shell.load_paths = self.load_paths;
        shell.syntax_only = false;
        let Value::List(mut bundle) = nodes.value(&self.graph.value) else {
            unreachable!()
        };
        let Value::List(values) = bundle.pop().unwrap() else {
            unreachable!()
        };
        (bundle.pop().unwrap(), values)
    }
}
