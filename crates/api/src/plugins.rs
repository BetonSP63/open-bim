//! Plugins: programs that add objects to the model by their own rules.
//!
//! A plugin is a folder with `plugin.json`:
//!
//! ```json
//! { "name": "Ряд колонн", "kind": "model", "program": "columns",
//!   "inputs": [ { "name": "start", "type": "point", "prompt": "Первая колонна" },
//!               { "name": "end", "type": "point", "prompt": "Последняя колонна" } ],
//!   "attributes": [ { "name": "count", "label": "Количество", "type": "number", "default": 4 } ] }
//! ```
//!
//! The contract has two parts, as in the components of the program:
//! * the **input definition** — at most ten inputs (points or parts) the
//!   person picks, and named attributes with defaults (the dialog);
//! * the **run** — the program gets the inputs, the attributes and the
//!   current model as JSON on stdin and answers on stdout with macro steps
//!   (`{"steps": [...]}`) or `{"error": "..."}`.
//!
//! The program runs in its own process with the plugin folder as working
//! directory and a time limit; it never touches the model or files of the
//! project itself. Its steps go through the ordinary checks of the session
//! and are staged together: if one fails, none is kept. Only model and
//! drawing edits are allowed — no commit, save, export or nested plugins.
use crate::batch::{self, Step};
use crate::{ApiError, Result, Session, invalid};
use open_bim_commands::Command as ModelCommand;
use open_bim_model::{ObjectId, PluginInput, PluginInstance, PluginValue, Point3, Project};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const MANIFEST: &str = "plugin.json";
/// Inputs a plugin may ask for.
pub const MAX_INPUTS: usize = 10;
/// Length limit of a plugin code (short identifier shown on its symbol).
pub const MAX_CODE_CHARS: usize = 22;
const MAX_OUTPUT_BYTES: usize = 16 << 20;
const DEFAULT_TIMEOUT_S: u64 = 60;
pub const PROTOCOL: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// Adds or changes model objects.
    Model,
    /// Joins parts (gets the parts it connects as inputs).
    Connection,
    /// Works on drawings.
    Drawing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    Point,
    Points,
    Part,
    Parts,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputDef {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: InputType,
    /// Hint shown while picking.
    #[serde(default)]
    pub prompt: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeType {
    Number,
    Text,
    /// Profile name from the catalogue.
    Profile,
    /// Steel grade.
    Material,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributeDef {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(rename = "type")]
    pub kind: AttributeType,
    pub default: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub kind: PluginKind,
    /// Short code (at most 22 characters), e.g. «КОЛ-РЯД».
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub description: String,
    /// Executable, relative to the plugin folder or absolute.
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<InputDef>,
    #[serde(default)]
    pub attributes: Vec<AttributeDef>,
    /// Time limit of one run, seconds.
    #[serde(default = "default_timeout")]
    pub timeout_s: u64,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_S
}

/// A plugin found on disk.
#[derive(Clone, Debug, PartialEq)]
pub struct Plugin {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

impl Manifest {
    pub fn validate(&self) -> Result<()> {
        let name_ok = |s: &str| !s.trim().is_empty() && s.chars().count() <= 64;
        if !name_ok(&self.name) {
            return invalid("Имя плагина: от 1 до 64 символов");
        }
        if let Some(code) = &self.code
            && (code.trim().is_empty() || code.chars().count() > MAX_CODE_CHARS)
        {
            return invalid(format!("Код плагина: от 1 до {MAX_CODE_CHARS} символов"));
        }
        if self.program.trim().is_empty() {
            return invalid("Не задана программа плагина");
        }
        if self.inputs.len() > MAX_INPUTS {
            return invalid(format!("Плагину можно не больше {MAX_INPUTS} входов"));
        }
        if !(1..=3600).contains(&self.timeout_s) {
            return invalid("Время работы плагина: от 1 до 3600 с");
        }
        let mut names = BTreeSet::new();
        for name in self
            .inputs
            .iter()
            .map(|i| &i.name)
            .chain(self.attributes.iter().map(|a| &a.name))
        {
            if !name_ok(name) || !names.insert(name) {
                return invalid(format!("Плагин: пустое или повторное имя «{name}»"));
            }
        }
        for a in &self.attributes {
            check_attribute(a, &a.default)?;
        }
        Ok(())
    }
}

fn check_attribute(def: &AttributeDef, value: &serde_json::Value) -> Result<()> {
    let ok = match def.kind {
        AttributeType::Number => value.as_f64().is_some_and(f64::is_finite),
        _ => value.as_str().is_some_and(|s| s.len() <= 4096),
    };
    if ok {
        Ok(())
    } else {
        invalid(format!("Атрибут «{}»: неверное значение {value}", def.name))
    }
}

/// Plugins in the immediate subfolders of `dirs` (each with a
/// `plugin.json`), sorted by name; broken manifests are reported, not fatal.
pub fn discover(dirs: &[PathBuf]) -> (Vec<Plugin>, Vec<String>) {
    let mut plugins = Vec::new();
    let mut problems = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut folders: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.join(MANIFEST).is_file())
            .collect();
        folders.sort();
        for folder in folders {
            match load(&folder) {
                Ok(plugin) => plugins.push(plugin),
                Err(e) => problems.push(format!("{}: {e}", folder.display())),
            }
        }
    }
    plugins.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    let mut seen = BTreeSet::new();
    plugins.retain(|p| {
        let first = seen.insert(p.manifest.name.clone());
        if !first {
            problems.push(format!(
                "{}: плагин «{}» уже найден в другой папке",
                p.dir.display(),
                p.manifest.name
            ));
        }
        first
    });
    (plugins, problems)
}

pub fn load(folder: &Path) -> Result<Plugin> {
    let path = folder.join(MANIFEST);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ApiError::Io(format!("{}: {e}", path.display())))?;
    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|e| ApiError::Invalid(format!("{}: {e}", path.display())))?;
    manifest.validate()?;
    Ok(Plugin {
        dir: folder.to_path_buf(),
        manifest,
    })
}

/// A picked input value; sent to the program as plain JSON (`[x, y, z]`,
/// a list of points, an ID or a list of IDs).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum InputValue {
    Point(Point3),
    Points(Vec<Point3>),
    Part(ObjectId),
    Parts(Vec<ObjectId>),
}

impl From<&InputValue> for PluginInput {
    fn from(value: &InputValue) -> Self {
        match value {
            InputValue::Point(p) => Self::Point(*p),
            InputValue::Points(v) => Self::Points(v.clone()),
            InputValue::Part(id) => Self::Part(*id),
            InputValue::Parts(ids) => Self::Parts(ids.clone()),
        }
    }
}

impl From<&PluginInput> for InputValue {
    fn from(value: &PluginInput) -> Self {
        match value {
            PluginInput::Point(p) => Self::Point(*p),
            PluginInput::Points(v) => Self::Points(v.clone()),
            PluginInput::Part(id) => Self::Part(*id),
            PluginInput::Parts(ids) => Self::Parts(ids.clone()),
        }
    }
}

fn stored(value: &serde_json::Value) -> Option<PluginValue> {
    match value {
        serde_json::Value::Number(n) => n.as_f64().map(PluginValue::Number),
        serde_json::Value::String(s) => Some(PluginValue::Text(s.clone())),
        _ => None,
    }
}

fn json(value: &PluginValue) -> serde_json::Value {
    match value {
        PluginValue::Number(n) => serde_json::json!(n),
        PluginValue::Text(s) => serde_json::Value::String(s.clone()),
    }
}

impl InputValue {
    /// Read JSON as the declared input type (a list of three IDs and a
    /// point look the same in JSON).
    pub fn from_json(kind: InputType, value: &serde_json::Value) -> Result<Self> {
        let parsed = match kind {
            InputType::Point => serde_json::from_value(value.clone()).map(Self::Point),
            InputType::Points => serde_json::from_value(value.clone()).map(Self::Points),
            InputType::Part => serde_json::from_value(value.clone()).map(Self::Part),
            InputType::Parts => serde_json::from_value::<Vec<ObjectId>>(value.clone())
                .or_else(|_| serde_json::from_value(value.clone()).map(|id| vec![id]))
                .map(Self::Parts),
        };
        parsed.map_err(|e| ApiError::Invalid(format!("Вход {kind:?}: {e}")))
    }
}

#[derive(Serialize)]
struct Request<'a> {
    protocol: u32,
    plugin: &'a str,
    inputs: &'a BTreeMap<String, InputValue>,
    attributes: BTreeMap<String, serde_json::Value>,
    model: &'a open_bim_model::Project,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    #[serde(default)]
    steps: Vec<Step>,
    #[serde(default)]
    error: Option<String>,
}

impl Plugin {
    /// Check picked inputs against the definition; numbers of the same JSON
    /// shape are told apart by the declared type.
    fn inputs(
        &self,
        given: &BTreeMap<String, InputValue>,
        session: &Session,
    ) -> Result<BTreeMap<String, InputValue>> {
        let mut out = BTreeMap::new();
        for def in &self.manifest.inputs {
            let value = given
                .get(&def.name)
                .ok_or_else(|| ApiError::Invalid(format!("Не указан вход «{}»", def.name)))?;
            let value = match (def.kind, value) {
                (InputType::Point, InputValue::Point(p)) => InputValue::Point(*p),
                (InputType::Points, InputValue::Points(v)) if !v.is_empty() => {
                    InputValue::Points(v.clone())
                }
                (InputType::Part, InputValue::Part(id)) => InputValue::Part(*id),
                (InputType::Parts, InputValue::Parts(ids)) if !ids.is_empty() => {
                    InputValue::Parts(ids.clone())
                }
                (InputType::Parts, InputValue::Part(id)) => InputValue::Parts(vec![*id]),
                _ => return invalid(format!("Вход «{}»: ожидается {:?}", def.name, def.kind)),
            };
            for id in id_of(&value) {
                if !session.project().elements.contains_key(&id) {
                    return Err(ApiError::NotFound(id));
                }
            }
            out.insert(def.name.clone(), value);
        }
        if let Some(extra) = given.keys().find(|k| !out.contains_key(*k)) {
            return invalid(format!(
                "Плагин «{}» не знает входа «{extra}»",
                self.manifest.name
            ));
        }
        Ok(out)
    }

    fn attributes(
        &self,
        given: &BTreeMap<String, serde_json::Value>,
    ) -> Result<BTreeMap<String, serde_json::Value>> {
        let mut out = BTreeMap::new();
        for def in &self.manifest.attributes {
            let value = given.get(&def.name).unwrap_or(&def.default);
            check_attribute(def, value)?;
            out.insert(def.name.clone(), value.clone());
        }
        if let Some(extra) = given.keys().find(|k| !out.contains_key(*k)) {
            return invalid(format!(
                "Плагин «{}» не знает атрибута «{extra}»",
                self.manifest.name
            ));
        }
        Ok(out)
    }

    fn program(&self) -> PathBuf {
        let program = Path::new(&self.manifest.program);
        if program.is_absolute() {
            program.to_path_buf()
        } else {
            self.dir.join(program)
        }
    }

    /// Run the program and parse its answer (no change to the session).
    fn call(&self, request: &[u8]) -> Result<Vec<Step>> {
        let name = &self.manifest.name;
        let fail = |m: String| ApiError::Invalid(format!("Плагин «{name}»: {m}"));
        let mut child = Command::new(self.program())
            .args(&self.manifest.args)
            .current_dir(&self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| fail(format!("не запускается: {e}")))?;
        let mut stdin = child.stdin.take().ok_or_else(|| fail("нет stdin".into()))?;
        let request = request.to_vec();
        // Separate threads so a program that writes before reading all of
        // its input cannot block on a full pipe.
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&request);
        });
        let read_limited = |pipe: Option<Box<dyn Read + Send>>| {
            std::thread::spawn(move || {
                let mut out = Vec::new();
                if let Some(pipe) = pipe {
                    let _ = pipe.take(MAX_OUTPUT_BYTES as u64 + 1).read_to_end(&mut out);
                }
                out
            })
        };
        let stdout = read_limited(child.stdout.take().map(|p| Box::new(p) as _));
        let stderr = read_limited(child.stderr.take().map(|p| Box::new(p) as _));
        let deadline = Instant::now() + Duration::from_secs(self.manifest.timeout_s);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(fail(format!(
                        "не уложился в {} с и остановлен",
                        self.manifest.timeout_s
                    )));
                }
                Err(e) => return Err(fail(e.to_string())),
            }
        };
        let _ = writer.join();
        let out = stdout.join().unwrap_or_default();
        let err = stderr.join().unwrap_or_default();
        if out.len() > MAX_OUTPUT_BYTES {
            return Err(fail("слишком большой ответ".into()));
        }
        if !status.success() {
            let message = String::from_utf8_lossy(&err);
            let message = message.trim();
            return Err(fail(if message.is_empty() {
                format!("завершился с ошибкой ({status})")
            } else {
                message.chars().take(2000).collect()
            }));
        }
        let response: Response =
            serde_json::from_slice(&out).map_err(|e| fail(format!("неверный ответ: {e}")))?;
        if let Some(error) = response.error {
            return Err(fail(error));
        }
        for step in &response.steps {
            if !allowed(step) {
                return Err(fail(format!(
                    "шаг «{}» плагину недоступен",
                    serde_json::to_value(step)
                        .ok()
                        .and_then(|v| v.as_object()?.keys().next().cloned())
                        .or_else(|| serde_json::to_value(step).ok()?.as_str().map(String::from))
                        .unwrap_or_default()
                )));
            }
        }
        Ok(response.steps)
    }
}

fn id_of(value: &InputValue) -> Vec<ObjectId> {
    match value {
        InputValue::Part(id) => vec![*id],
        InputValue::Parts(ids) => ids.clone(),
        _ => Vec::new(),
    }
}

fn allowed(step: &Step) -> bool {
    matches!(
        step,
        Step::InsertMember(_)
            | Step::InsertPlate(_)
            | Step::ModifyMember(_)
            | Step::ModifyPlate(_)
            | Step::Select(_)
            | Step::Delete { .. }
            | Step::SetProperty { .. }
            | Step::Move { .. }
            | Step::Copy { .. }
            | Step::CreateDrawing(_)
    )
}

impl Session {
    /// Run a plugin with picked inputs and attribute values (missing
    /// attributes take their defaults). Its steps and the placed plugin
    /// object that remembers them are staged together; returns the IDs of
    /// the created objects. Nothing is staged on error.
    pub fn run_plugin(
        &mut self,
        plugin: &Plugin,
        inputs: &BTreeMap<String, InputValue>,
        attributes: &BTreeMap<String, serde_json::Value>,
    ) -> Result<Vec<ObjectId>> {
        self.refuse_any_open("запустить плагин")?;
        let snapshot = (self.staged.len(), self.working.clone());
        let result = self
            .call_plugin(plugin, inputs, attributes, &BTreeSet::new())
            .and_then(|(inputs, attributes, created)| {
                let instance = PluginInstance {
                    id: ObjectId(self.working.next_id),
                    plugin: plugin.manifest.name.clone(),
                    inputs: inputs.iter().map(|(k, v)| (k.clone(), v.into())).collect(),
                    attributes,
                    outputs: created.iter().copied().collect(),
                };
                self.stage(vec![ModelCommand::CreatePlugin { instance }])?;
                Ok(created)
            });
        if result.is_err() {
            self.restore(snapshot);
        }
        result
    }

    /// Placed plugins of the model.
    pub fn plugin_instances(&self) -> impl Iterator<Item = &PluginInstance> {
        self.working.plugins.values()
    }

    /// Run a placed plugin again with its stored inputs (and new attribute
    /// values when given). Its outputs are replaced: objects made again keep
    /// their IDs in creation order, the rest are deleted. Nothing is staged
    /// on error.
    pub fn rerun_plugin(
        &mut self,
        plugin: &Plugin,
        id: ObjectId,
        attributes: Option<&BTreeMap<String, serde_json::Value>>,
    ) -> Result<Vec<ObjectId>> {
        self.refuse_any_open("запустить плагин")?;
        let instance = self
            .working
            .plugins
            .get(&id)
            .cloned()
            .ok_or(ApiError::NotFound(id))?;
        if instance.plugin != plugin.manifest.name {
            return invalid(format!(
                "Объект {id} создан плагином «{}», а не «{}»",
                instance.plugin, plugin.manifest.name
            ));
        }
        let inputs: BTreeMap<String, InputValue> = instance
            .inputs
            .iter()
            .map(|(k, v)| (k.clone(), v.into()))
            .collect();
        let attributes = match attributes {
            Some(given) => given.clone(),
            None => instance
                .attributes
                .iter()
                .map(|(k, v)| (k.clone(), json(v)))
                .collect(),
        };
        let snapshot = (self.staged.len(), self.working.clone());
        self.reuse = instance
            .outputs
            .iter()
            .filter(|o| self.working.elements.contains_key(o))
            .copied()
            .collect();
        let result = self
            .stage(vec![ModelCommand::ReplacePlugin {
                instance: PluginInstance {
                    outputs: BTreeSet::new(),
                    ..instance.clone()
                },
            }])
            .and_then(|()| self.call_plugin(plugin, &inputs, &attributes, &instance.outputs))
            .and_then(|(_, attributes, created)| {
                let mut commands: Vec<ModelCommand> = instance
                    .outputs
                    .iter()
                    .filter(|o| !created.contains(o) && self.working.contains_id(**o))
                    .map(|&id| ModelCommand::Delete {
                        id,
                        policy: open_bim_commands::DeletePolicy::Cascade,
                    })
                    .collect();
                commands.push(ModelCommand::ReplacePlugin {
                    instance: PluginInstance {
                        attributes,
                        outputs: created.iter().copied().collect(),
                        ..instance.clone()
                    },
                });
                self.stage(commands)?;
                Ok(created)
            });
        self.reuse.clear();
        if result.is_err() {
            self.restore(snapshot);
        }
        result
    }

    fn restore(&mut self, (staged, working): (usize, Project)) {
        self.staged.truncate(staged);
        self.working = working;
    }

    /// Call the program and stage its steps; returns the checked inputs,
    /// the attribute values used and the created objects still present.
    /// `hidden` objects (the outputs of the previous run) are left out of
    /// the model the program sees.
    #[allow(clippy::type_complexity)]
    fn call_plugin(
        &mut self,
        plugin: &Plugin,
        inputs: &BTreeMap<String, InputValue>,
        attributes: &BTreeMap<String, serde_json::Value>,
        hidden: &BTreeSet<ObjectId>,
    ) -> Result<(
        BTreeMap<String, InputValue>,
        BTreeMap<String, PluginValue>,
        Vec<ObjectId>,
    )> {
        let inputs = plugin.inputs(inputs, self)?;
        let attributes = plugin.attributes(attributes)?;
        let mut model = std::borrow::Cow::Borrowed(self.project());
        if !hidden.is_empty() {
            let m = model.to_mut();
            m.elements.retain(|id, _| !hidden.contains(id));
            m.drawings.retain(|id, _| !hidden.contains(id));
        }
        let request = Request {
            protocol: PROTOCOL,
            plugin: &plugin.manifest.name,
            inputs: &inputs,
            attributes: attributes.clone(),
            model: &model,
        };
        let request = serde_json::to_vec(&request).map_err(|e| ApiError::Invalid(e.to_string()))?;
        let steps = plugin.call(&request)?;
        let created = batch::run_plugin_steps(self, &steps, &plugin.dir).map_err(|e| match e {
            ApiError::Invalid(m) => {
                ApiError::Invalid(format!("Плагин «{}»: {m}", plugin.manifest.name))
            }
            other => other,
        })?;
        let mut seen = BTreeSet::new();
        let created = created
            .into_iter()
            .filter(|id| self.working.contains_id(*id) && seen.insert(*id))
            .collect();
        let attributes = attributes
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), stored(v)?)))
            .collect();
        Ok((inputs, attributes, created))
    }

    /// Placed plugins that depend on any of `touched` (an input part
    /// changed), in ID order.
    pub fn dependent_plugins(project: &Project, touched: &BTreeSet<ObjectId>) -> Vec<ObjectId> {
        project
            .plugins
            .values()
            .filter(|p| !p.input_parts().is_disjoint(touched))
            .map(|p| p.id)
            .collect()
    }

    /// Run again every placed plugin whose input parts are in `touched`,
    /// with the plugins known to the session. A plugin that is missing or
    /// fails keeps its old outputs; its problem is returned.
    pub fn rerun_dependent(&mut self, touched: &BTreeSet<ObjectId>) -> Vec<String> {
        let mut problems = Vec::new();
        let mut touched = touched.clone();
        // ID order: a plugin placed on another plugin's outputs comes later
        // and sees them already rebuilt.
        let ids: Vec<ObjectId> = self.working.plugins.keys().copied().collect();
        for id in ids {
            let Some(instance) = self.working.plugins.get(&id) else {
                continue;
            };
            if instance.input_parts().is_disjoint(&touched) {
                continue;
            }
            touched.extend(&instance.outputs);
            let Some(name) = self.working.plugins.get(&id).map(|p| p.plugin.clone()) else {
                continue;
            };
            let Some(plugin) = self
                .plugins
                .iter()
                .find(|p| p.manifest.name == name)
                .cloned()
            else {
                problems.push(format!("Плагин «{name}» ({id}) не найден и не обновлён"));
                continue;
            };
            match self.rerun_plugin(&plugin, id, None) {
                Ok(created) => touched.extend(created),
                Err(e) => problems.push(format!("{id}: {e}")),
            }
        }
        problems
    }
}
