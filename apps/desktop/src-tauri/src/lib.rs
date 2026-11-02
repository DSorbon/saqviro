use saqviro_core::{manifest::ToolManifest, registry::Registry};
use tauri::State;

mod catalog {
    include!(concat!(env!("OUT_DIR"), "/manifests.rs"));
}

fn bundled_registry() -> Registry {
    Registry::from_sources(catalog::MANIFESTS.iter().copied())
        .unwrap_or_else(|e| panic!("bundled tool manifests are invalid: {e}"))
}

#[tauri::command]
fn list_tools(registry: State<'_, Registry>) -> Vec<ToolManifest> {
    registry.tools().to_vec()
}

/// Smart Drop Lite: ids of tools that accept every dropped file.
#[tauri::command]
fn tools_for_files(registry: State<'_, Registry>, paths: Vec<String>) -> Vec<String> {
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    registry
        .for_files(&paths)
        .into_iter()
        .map(|t| t.id.to_string())
        .collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(bundled_registry())
        .invoke_handler(tauri::generate_handler![list_tools, tools_for_files])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_manifests_load() {
        let registry = bundled_registry();
        assert_eq!(registry.tools().len(), catalog::MANIFESTS.len());
        assert!(registry.get("image.resize").is_some());
    }
}
