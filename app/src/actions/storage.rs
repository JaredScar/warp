use std::io;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{anyhow, Result};

use super::model::{Action, SavedWorkspace, Trigger};

// ── Directory paths ───────────────────────────────────────────────────────────

pub fn actions_dir() -> PathBuf {
    warp_core::paths::data_dir().join("actions")
}

pub fn triggers_dir() -> PathBuf {
    warp_core::paths::data_dir().join("triggers")
}

pub fn workspaces_dir() -> PathBuf {
    warp_core::paths::data_dir().join("workspaces")
}

// ── Save helpers ──────────────────────────────────────────────────────────────

/// Serialize `action` to `~/.warp/actions/<slug>.toml`.
pub fn save_action(action: &Action) -> Result<PathBuf> {
    let dir = actions_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.toml", slug(&action.name)));
    if path.exists() {
        return Err(anyhow!("File already exists: {}", path.display()));
    }
    let toml = toml::to_string_pretty(action)?;
    let file = crate::util::file::create_file(path.clone())?;
    let mut writer = io::BufWriter::new(file);
    writer.write_all(toml.as_bytes())?;
    Ok(path)
}

/// Serialize `trigger` to `~/.warp/triggers/<slug>.toml`.
pub fn save_trigger(trigger: &Trigger) -> Result<PathBuf> {
    let dir = triggers_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.toml", slug(&trigger.name)));
    if path.exists() {
        return Err(anyhow!("File already exists: {}", path.display()));
    }
    let toml = toml::to_string_pretty(trigger)?;
    let file = crate::util::file::create_file(path.clone())?;
    let mut writer = io::BufWriter::new(file);
    writer.write_all(toml.as_bytes())?;
    Ok(path)
}

/// Serialize `workspace` to `~/.warp/workspaces/<slug>.toml`.
pub fn save_workspace(workspace: &SavedWorkspace) -> Result<PathBuf> {
    let dir = workspaces_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.toml", slug(&workspace.name)));
    if path.exists() {
        return Err(anyhow!("File already exists: {}", path.display()));
    }
    let toml = toml::to_string_pretty(workspace)?;
    let file = crate::util::file::create_file(path.clone())?;
    let mut writer = io::BufWriter::new(file);
    writer.write_all(toml.as_bytes())?;
    Ok(path)
}

/// Overwrite an existing action file (or create a new one keyed by UUID).
///
/// Use this when editing an existing action: delete the old file first if the
/// name changed, then call `write_action` to write the updated content.
pub fn write_action(action: &Action) -> Result<PathBuf> {
    let path = action
        .source_path
        .clone()
        .unwrap_or_else(|| actions_dir().join(format!("{}.toml", action.id)));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml = toml::to_string_pretty(action)?;
    std::fs::write(&path, toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write action '{}': {e}", path.display()))?;
    Ok(path)
}

/// Overwrite an existing trigger file (or create a new one keyed by UUID).
pub fn write_trigger(trigger: &Trigger) -> Result<PathBuf> {
    let path = trigger
        .source_path
        .clone()
        .unwrap_or_else(|| triggers_dir().join(format!("{}.toml", trigger.id)));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml = toml::to_string_pretty(trigger)?;
    std::fs::write(&path, toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write trigger '{}': {e}", path.display()))?;
    Ok(path)
}

// ── Delete helpers ────────────────────────────────────────────────────────────

/// Delete the TOML file backing `action` (uses `source_path` when present,
/// otherwise derives the path from the action's name).
pub fn delete_action(action: &Action) -> Result<()> {
    let path = action
        .source_path
        .clone()
        .unwrap_or_else(|| actions_dir().join(format!("{}.toml", slug(&action.name))));
    std::fs::remove_file(&path)
        .map_err(|e| anyhow::anyhow!("Failed to delete action '{}': {e}", path.display()))
}

/// Delete the TOML file backing `trigger`.
pub fn delete_trigger(trigger: &Trigger) -> Result<()> {
    let path = trigger
        .source_path
        .clone()
        .unwrap_or_else(|| triggers_dir().join(format!("{}.toml", slug(&trigger.name))));
    std::fs::remove_file(&path)
        .map_err(|e| anyhow::anyhow!("Failed to delete trigger '{}': {e}", path.display()))
}

/// Delete the TOML file backing `workspace`.
pub fn delete_workspace(workspace: &SavedWorkspace) -> Result<()> {
    let path = workspace
        .source_path
        .clone()
        .unwrap_or_else(|| workspaces_dir().join(format!("{}.toml", slug(&workspace.name))));
    std::fs::remove_file(&path)
        .map_err(|e| anyhow::anyhow!("Failed to delete workspace '{}': {e}", path.display()))
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect()
}
