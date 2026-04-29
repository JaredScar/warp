use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A named group of shell commands that can be run together.
///
/// Actions are the atomic unit of automation — each holds an ordered list of
/// shell commands that will be sent to a terminal in sequence.  Multiple
/// actions can be composed into a [`Trigger`].
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Action {
    /// Stable identifier used to reference this action from triggers.
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    /// Human-readable name shown in the panel and picker.
    pub name: String,
    /// Optional description shown as secondary text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Shell commands to run in order.  Each entry is a single command string
    /// that will be sent to the target terminal exactly as written.
    #[serde(default)]
    pub commands: Vec<String>,
    /// Absolute path of the TOML file this action was loaded from.
    /// Skipped during serialisation — it is set by the loader.
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

/// Specifies which open terminal panes a [`Trigger`] should target.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TriggerTargets {
    /// Send commands to every open terminal pane in the active tab.
    #[default]
    AllOpen,
    /// Send commands to panes at the given 0-based tab indices.
    ByIndex { indices: Vec<usize> },
    /// Send commands to panes whose title or current working directory
    /// contains one of the given substrings (case-insensitive).
    ByTitle { titles: Vec<String> },
}

/// An ordered sequence of [`Action`]s that run against one or more terminals.
///
/// When a trigger fires it executes each action in `action_ids` in order.
/// Within each action, commands are dispatched concurrently to all resolved
/// terminal targets so that multiple panes progress in lock-step.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Trigger {
    /// Stable identifier for this trigger.
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    /// Human-readable name shown in the panel.
    pub name: String,
    /// Optional description shown as secondary text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Ordered list of [`Action`] IDs to execute.
    #[serde(default)]
    pub action_ids: Vec<Uuid>,
    /// Which terminal panes this trigger targets.
    #[serde(default)]
    pub targets: TriggerTargets,
    /// Absolute path of the TOML file this trigger was loaded from.
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

/// A user-named snapshot of the current window layout (tabs, pane splits,
/// shell working directories) that can be restored later.
///
/// Stored under `~/.warp/workspaces/*.toml`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SavedWorkspace {
    /// Stable identifier for this workspace.
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    /// Human-readable name shown in the Workspaces tab.
    pub name: String,
    /// The full window snapshot captured at save time.
    pub snapshot: WorkspaceSnapshot,
    /// Absolute path of the TOML file this workspace was loaded from.
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

/// A serialisable representation of one tab's pane layout for workspace
/// save/restore.  Mirrors the fields of [`WindowSnapshot`] that are stable
/// enough to round-trip through TOML.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct WorkspaceSnapshot {
    /// Ordered list of tab snapshots.
    pub tabs: Vec<WorkspaceTabSnapshot>,
    /// Index of the tab that was active at save time.
    #[serde(default)]
    pub active_tab_index: usize,
}

/// Snapshot of a single tab sufficient for restore.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct WorkspaceTabSnapshot {
    /// Custom tab title, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_title: Option<String>,
    /// Working directory of the first/focused pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Shell command to run on open (forwarded to `ShellLaunchData`).
    #[serde(default)]
    pub commands: Vec<String>,
}

impl WorkspaceSnapshot {
    /// Build a lightweight snapshot from the live [`crate::app_state::WindowSnapshot`].
    pub fn from_window_snapshot(ws: &crate::app_state::WindowSnapshot) -> Self {
        use crate::app_state::{LeafContents, PaneNodeSnapshot, TerminalPaneSnapshot};

        fn first_terminal(node: &PaneNodeSnapshot) -> Option<&TerminalPaneSnapshot> {
            match node {
                PaneNodeSnapshot::Leaf(leaf) => {
                    if let LeafContents::Terminal(t) = &leaf.contents {
                        Some(t)
                    } else {
                        None
                    }
                }
                PaneNodeSnapshot::Branch(branch) => branch
                    .children
                    .iter()
                    .find_map(|(_, child)| first_terminal(child)),
            }
        }

        let tabs = ws
            .tabs
            .iter()
            .map(|tab| {
                let terminal = first_terminal(&tab.root);
                WorkspaceTabSnapshot {
                    custom_title: tab.custom_title.clone(),
                    cwd: terminal.and_then(|t| t.cwd.clone()),
                    commands: vec![],
                }
            })
            .collect();

        WorkspaceSnapshot {
            tabs,
            active_tab_index: ws.active_tab_index,
        }
    }
}
