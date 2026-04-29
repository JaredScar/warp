use std::io;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use uuid::Uuid;

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

// ── Private helpers ───────────────────────────────────────────────────────────

fn slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect()
}
