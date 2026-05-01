use std::io;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use uuid::Uuid;

use super::model::{Action, Runbook, SavedWorkspace, Trigger, TriggerHistory};

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

pub fn trigger_history_dir() -> PathBuf {
    warp_core::paths::data_dir().join("trigger_history")
}

pub fn runbooks_dir() -> PathBuf {
    warp_core::paths::data_dir().join("runbooks")
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

/// Overwrite an existing workspace file (or create a new one keyed by UUID).
///
/// Use this when renaming or updating an existing workspace: delete the old
/// file first if the name changed, then call `write_workspace`.
pub fn write_workspace(workspace: &SavedWorkspace) -> Result<PathBuf> {
    let path = workspace
        .source_path
        .clone()
        .unwrap_or_else(|| workspaces_dir().join(format!("{}.toml", workspace.id)));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml = toml::to_string_pretty(workspace)?;
    std::fs::write(&path, toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write workspace '{}': {e}", path.display()))?;
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

// ── Trigger history helpers ───────────────────────────────────────────────────

/// Load the run history for `trigger_id` from
/// `~/.warp/trigger_history/<trigger_id>.toml`.
///
/// Returns an empty [`TriggerHistory`] if the file does not exist or cannot
/// be parsed — history corruption is non-fatal.
pub fn load_trigger_history(trigger_id: Uuid) -> TriggerHistory {
    let path = trigger_history_dir().join(format!("{trigger_id}.toml"));
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return TriggerHistory::default();
    };
    toml::from_str::<TriggerHistory>(&contents).unwrap_or_default()
}

/// Persist `history` for `trigger_id` to
/// `~/.warp/trigger_history/<trigger_id>.toml`.
pub fn save_trigger_history(trigger_id: Uuid, history: &TriggerHistory) -> Result<()> {
    let dir = trigger_history_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{trigger_id}.toml"));
    let toml = toml::to_string_pretty(history)?;
    std::fs::write(&path, toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write trigger history '{}': {e}", path.display()))
}

// ── Runbook helpers ───────────────────────────────────────────────────────────

/// Write (create or overwrite) a runbook to `~/.warp/runbooks/<id>.toml`.
pub fn write_runbook(runbook: &Runbook) -> Result<PathBuf> {
    let path = runbook
        .source_path
        .clone()
        .unwrap_or_else(|| runbooks_dir().join(format!("{}.toml", runbook.id)));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml = toml::to_string_pretty(runbook)?;
    std::fs::write(&path, toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write runbook '{}': {e}", path.display()))?;
    Ok(path)
}

/// Delete the TOML file backing `runbook`.
pub fn delete_runbook(runbook: &Runbook) -> Result<()> {
    let path = runbook
        .source_path
        .clone()
        .unwrap_or_else(|| runbooks_dir().join(format!("{}.toml", runbook.id)));
    std::fs::remove_file(&path)
        .map_err(|e| anyhow::anyhow!("Failed to delete runbook '{}': {e}", path.display()))
}

/// Load all runbooks from `dir`.  Malformed or missing files are skipped.
pub fn load_runbooks(dir: &std::path::Path) -> Vec<Runbook> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut runbooks: Vec<Runbook> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()?.to_str()? != "toml" {
                return None;
            }
            let contents = std::fs::read_to_string(&path).ok()?;
            let mut runbook: Runbook = toml::from_str(&contents)
                .map_err(|e| log::warn!("Skipping malformed runbook {:?}: {e}", path))
                .ok()?;
            runbook.source_path = Some(path);
            Some(runbook)
        })
        .collect();
    runbooks.sort_by(|a, b| a.name.cmp(&b.name));
    runbooks
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect()
}
