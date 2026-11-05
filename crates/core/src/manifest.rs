use std::{borrow::Cow, collections::BTreeMap, fmt};

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_VERSION: u32 = 1;
const MAX_PRESETS: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolManifest {
    pub schema_version: u32,
    pub id: ToolId,
    pub name: String,
    pub category: Category,
    pub tool_version: u32,
    pub description: String,
    pub keywords: Vec<String>,
    pub inputs: Vec<InputPort>,
    pub outputs: Vec<OutputPort>,
    pub engine: String,
    pub capabilities: Capabilities,
    pub permissions: Permissions,
    pub platforms: Vec<Platform>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    pub presets: Vec<Preset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiHints>,
}

/// `category.operation[.qualifier]`, lowercase ASCII, each segment starting with a letter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ToolId(String);

impl ToolId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn category(&self) -> &str {
        self.0.split('.').next().unwrap_or_default()
    }
}

impl TryFrom<String> for ToolId {
    type Error = String;

    fn try_from(s: String) -> Result<Self, String> {
        let segments: Vec<&str> = s.split('.').collect();
        let valid_segment = |seg: &str| {
            seg.starts_with(|c: char| c.is_ascii_lowercase())
                && seg
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        };
        if !(2..=3).contains(&segments.len()) || !segments.iter().all(|s| valid_segment(s)) {
            return Err(format!(
                "invalid tool id `{s}`: expected `category.operation[.qualifier]` in lowercase"
            ));
        }
        Ok(Self(s))
    }
}

impl From<ToolId> for String {
    fn from(id: ToolId) -> Self {
        id.0
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl JsonSchema for ToolId {
    fn schema_name() -> Cow<'static, str> {
        "ToolId".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^[a-z][a-z0-9-]*(\\.[a-z][a-z0-9-]*){1,2}$"
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Pdf,
    Image,
    Video,
    Audio,
    Files,
    Text,
    Qr,
    Developer,
    Privacy,
    Network,
    System,
    Screen,
    Ocr,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Files => "files",
            Self::Text => "text",
            Self::Qr => "qr",
            Self::Developer => "developer",
            Self::Privacy => "privacy",
            Self::Network => "network",
            Self::System => "system",
            Self::Screen => "screen",
            Self::Ocr => "ocr",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    Image,
    Pdf,
    Video,
    Audio,
    Archive,
    Text,
    Json,
    Any,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputPort {
    #[serde(rename = "type")]
    pub data_type: DataType,
    #[serde(default)]
    pub multiple: bool,
    /// Required for every type except `text`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mime: Vec<String>,
    /// Lowercase, without the leading dot. Required for every type except `text`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputPort {
    #[serde(rename = "type")]
    pub data_type: DataType,
    #[serde(default)]
    pub multiple: bool,
    /// Output file name template, e.g. `{stem}-small.{ext}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub batch: bool,
    pub preview: bool,
    pub offline: bool,
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    #[serde(default)]
    pub network: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    Integer,
    Number,
    Boolean,
    String,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub id: String,
    #[serde(rename = "type")]
    pub param_type: ParamType,
    #[serde(default)]
    pub optional: bool,
    /// The value is a password: the UI hides it. Only for `string`.
    #[serde(default)]
    pub secret: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Allowed values; required for `enum`, forbidden otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

impl Parameter {
    fn check(&self, value: &Value) -> Result<(), String> {
        let in_range = |n: f64| {
            if self.min.is_some_and(|min| n < min) || self.max.is_some_and(|max| n > max) {
                Err(format!("{n} is outside {:?}..={:?}", self.min, self.max))
            } else {
                Ok(())
            }
        };
        match (self.param_type, value) {
            (ParamType::Integer, Value::Number(n)) if n.is_i64() || n.is_u64() => {
                in_range(n.as_f64().unwrap_or_default())
            }
            (ParamType::Number, Value::Number(n)) => in_range(n.as_f64().unwrap_or_default()),
            (ParamType::Boolean, Value::Bool(_)) | (ParamType::String, Value::String(_)) => Ok(()),
            (ParamType::Enum, Value::String(s)) if self.values.contains(s) => Ok(()),
            (ParamType::Enum, _) => Err(format!("{value} is not one of {:?}", self.values)),
            _ => Err(format!("{value} is not a valid {:?}", self.param_type)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UiHints {
    /// Parameters hidden under the "Advanced" disclosure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advanced: Vec<String>,
    /// Custom preview widget, e.g. `image-compare`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub field: String,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("cannot parse manifest: {0}")]
    Parse(String),
    #[error("invalid manifest:{}", .0.iter().map(|i| format!("\n  - {i}")).collect::<String>())]
    Invalid(Vec<Issue>),
}

impl ToolManifest {
    pub fn from_yaml(src: &str) -> Result<Self, ManifestError> {
        let manifest: Self = serde_saphyr::from_str(src).map_err(|e| ManifestError::Parse(e.to_string()))?;
        let issues = manifest.validate();
        if issues.is_empty() {
            Ok(manifest)
        } else {
            Err(ManifestError::Invalid(issues))
        }
    }

    /// Rules that serde cannot express. Returns every problem, not just the first.
    pub fn validate(&self) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut issue = |field: String, message: String| issues.push(Issue { field, message });

        if self.schema_version != SCHEMA_VERSION {
            issue(
                "schema_version".into(),
                format!("unsupported version {}, expected {SCHEMA_VERSION}", self.schema_version),
            );
        }
        if self.id.category() != self.category.as_str() {
            issue(
                "id".into(),
                format!(
                    "`{}` must start with its category `{}`",
                    self.id,
                    self.category.as_str()
                ),
            );
        }
        for (field, empty) in [
            ("name", self.name.trim().is_empty()),
            ("description", self.description.trim().is_empty()),
            ("engine", self.engine.trim().is_empty()),
            ("keywords", self.keywords.is_empty()),
            ("inputs", self.inputs.is_empty()),
            ("outputs", self.outputs.is_empty()),
            ("platforms", self.platforms.is_empty()),
        ] {
            if empty {
                issue(field.into(), "must not be empty".into());
            }
        }

        for (i, input) in self.inputs.iter().enumerate() {
            if input.data_type != DataType::Text {
                if input.mime.is_empty() {
                    issue(format!("inputs[{i}].mime"), "required for file inputs".into());
                }
                if input.extensions.is_empty() {
                    issue(format!("inputs[{i}].extensions"), "required for file inputs".into());
                }
            }
            for ext in &input.extensions {
                if ext.starts_with('.') || ext.chars().any(|c| c.is_ascii_uppercase()) {
                    issue(
                        format!("inputs[{i}].extensions"),
                        format!("`{ext}` must be lowercase without a leading dot"),
                    );
                }
            }
        }

        let mut seen = Vec::new();
        for p in &self.parameters {
            if seen.contains(&p.id.as_str()) {
                issue(format!("parameters.{}", p.id), "duplicate parameter id".into());
            }
            seen.push(&p.id);
            match (p.param_type, p.values.is_empty()) {
                (ParamType::Enum, true) => issue(format!("parameters.{}.values", p.id), "required for enum".into()),
                (ParamType::Enum, false) | (_, true) => {}
                (_, false) => issue(format!("parameters.{}.values", p.id), "only allowed for enum".into()),
            }
            if p.secret && p.param_type != ParamType::String {
                issue(format!("parameters.{}.secret", p.id), "only allowed for string".into());
            }
            if let Some(default) = &p.default
                && let Err(e) = p.check(default)
            {
                issue(format!("parameters.{}.default", p.id), e);
            }
        }

        if self.presets.is_empty() || self.presets.len() > MAX_PRESETS {
            issue(
                "presets".into(),
                format!("expected 1 to {MAX_PRESETS} presets, found {}", self.presets.len()),
            );
        }
        for preset in &self.presets {
            for (key, value) in &preset.params {
                match self.parameter(key) {
                    None => issue(
                        format!("presets.{}.{key}", preset.id),
                        format!("unknown parameter `{key}`"),
                    ),
                    Some(p) => {
                        if let Err(e) = p.check(value) {
                            issue(format!("presets.{}.{key}", preset.id), e);
                        }
                    }
                }
            }
        }

        if let Some(ui) = &self.ui {
            for key in &ui.advanced {
                if self.parameter(key).is_none() {
                    issue("ui.advanced".into(), format!("unknown parameter `{key}`"));
                }
            }
        }

        issues
    }

    /// Checks run-time parameters against the manifest and fills in defaults.
    /// `null` leaves an optional parameter unset. Issues are sorted by field.
    pub fn resolve_params(
        &self,
        given: &serde_json::Map<String, Value>,
    ) -> Result<serde_json::Map<String, Value>, Vec<Issue>> {
        let mut issues = Vec::new();
        let mut resolved = serde_json::Map::new();

        for key in given.keys().filter(|k| self.parameter(k).is_none()) {
            issues.push(Issue {
                field: key.clone(),
                message: "unknown parameter".into(),
            });
        }
        for p in &self.parameters {
            match given.get(&p.id).filter(|v| !v.is_null()).or(p.default.as_ref()) {
                Some(value) => match p.check(value) {
                    Ok(()) => {
                        resolved.insert(p.id.clone(), value.clone());
                    }
                    Err(e) => issues.push(Issue {
                        field: p.id.clone(),
                        message: e,
                    }),
                },
                None if p.optional => {}
                None => issues.push(Issue {
                    field: p.id.clone(),
                    message: "required".into(),
                }),
            }
        }

        if issues.is_empty() {
            Ok(resolved)
        } else {
            issues.sort_by(|a, b| a.field.cmp(&b.field));
            Err(issues)
        }
    }

    pub fn parameter(&self, id: &str) -> Option<&Parameter> {
        self.parameters.iter().find(|p| p.id == id)
    }

    /// The first preset is the default. Validation guarantees there is one.
    pub fn default_preset(&self) -> &Preset {
        &self.presets[0]
    }

    pub fn accepts_extension(&self, ext: &str) -> bool {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        self.inputs.iter().any(|i| i.extensions.contains(&ext))
    }
}
