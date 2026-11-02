use std::{collections::HashMap, fmt, path::Path};

use crate::manifest::ToolManifest;

/// All tools known to the app, sorted by id.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    tools: Vec<ToolManifest>,
}

/// Every manifest that failed to load, with its source path.
#[derive(Debug)]
pub struct RegistryError {
    pub errors: Vec<(String, String)>,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} manifest error(s)", self.errors.len())?;
        for (path, error) in &self.errors {
            write!(f, "\n{path}: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for RegistryError {}

impl Registry {
    /// Loads `(path, yaml)` pairs. `path` is only used in error messages.
    pub fn from_sources<'a>(sources: impl IntoIterator<Item = (&'a str, &'a str)>) -> Result<Self, RegistryError> {
        let mut tools = Vec::new();
        let mut errors = Vec::new();
        let mut paths: HashMap<String, &str> = HashMap::new();

        for (path, src) in sources {
            match ToolManifest::from_yaml(src) {
                Ok(tool) => {
                    if let Some(first) = paths.insert(tool.id.to_string(), path) {
                        errors.push((
                            path.to_owned(),
                            format!("duplicate tool id `{}`, already defined in {first}", tool.id),
                        ));
                    } else {
                        tools.push(tool);
                    }
                }
                Err(e) => errors.push((path.to_owned(), e.to_string())),
            }
        }

        if !errors.is_empty() {
            return Err(RegistryError { errors });
        }
        tools.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Self { tools })
    }

    pub fn tools(&self) -> &[ToolManifest] {
        &self.tools
    }

    pub fn get(&self, id: &str) -> Option<&ToolManifest> {
        self.tools.iter().find(|t| t.id.as_str() == id)
    }

    /// Tools that accept a file with this extension — the basis of Smart Drop Lite.
    pub fn for_extension<'a>(&'a self, ext: &'a str) -> impl Iterator<Item = &'a ToolManifest> {
        self.tools.iter().filter(move |t| t.accepts_extension(ext))
    }

    /// Smart Drop Lite: tools that accept every dropped file, by extension.
    /// A file without an extension matches nothing.
    pub fn for_files(&self, paths: &[&str]) -> Vec<&ToolManifest> {
        let exts: Option<Vec<&str>> = paths
            .iter()
            .map(|p| Path::new(p).extension().and_then(|e| e.to_str()))
            .collect();
        match exts {
            Some(exts) if !exts.is_empty() => self
                .tools
                .iter()
                .filter(|t| exts.iter().all(|e| t.accepts_extension(e)))
                .collect(),
            _ => Vec::new(),
        }
    }
}
