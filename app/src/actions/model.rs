use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::terminal::ShellLaunchData;

// ── Built-in system actions ───────────────────────────────────────────────────

/// Stable UUID for the built-in "Close All Terminals" action.
pub const BUILTIN_CLOSE_ALL_TERMINALS_ID: Uuid =
    uuid::uuid!("00000000-0000-0000-0000-000000000001");
/// Stable UUID for the built-in "Kill All Terminal Processes" action.
pub const BUILTIN_KILL_ALL_PROCESSES_ID: Uuid =
    uuid::uuid!("00000000-0000-0000-0000-000000000002");

/// Returns the list of built-in actions that are always present and cannot
/// be deleted by the user.
pub fn builtin_actions() -> Vec<Action> {
    vec![
        Action {
            id: BUILTIN_CLOSE_ALL_TERMINALS_ID,
            name: "Close All Terminals".to_string(),
            description: Some("Close every open terminal tab".to_string()),
            commands: vec![],
            tab_name: None,
            group: None,
            timeout_secs: None,
            hotkey: None,
            pinned: false,
            source_path: None,
        },
        Action {
            id: BUILTIN_KILL_ALL_PROCESSES_ID,
            name: "Kill All Terminal Processes".to_string(),
            description: Some("Send SIGINT (Ctrl-C) to every running terminal process".to_string()),
            commands: vec![],
            tab_name: None,
            group: None,
            timeout_secs: None,
            hotkey: None,
            pinned: false,
            source_path: None,
        },
    ]
}

/// Returns `true` if `id` belongs to a built-in system action.
pub fn is_builtin_action(id: &Uuid) -> bool {
    *id == BUILTIN_CLOSE_ALL_TERMINALS_ID || *id == BUILTIN_KILL_ALL_PROCESSES_ID
}

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
    /// Optional name to apply to the terminal tab opened for this action.
    /// When set, the new tab is renamed to this value immediately after it opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_name: Option<String>,
    /// Optional folder/group name used to organise actions in the panel list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Maximum seconds to wait for each command to complete before advancing
    /// to the next item in the trigger queue.  When `None` the global default
    /// (5 s) is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    /// Optional keyboard shortcut label displayed next to the action name in
    /// the panel (e.g. `"⌘⇧R"`).  Display-only — not used for key registration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    /// When `true` the action appears in the Quick Launch strip at the top of
    /// the Actions panel.
    #[serde(default)]
    pub pinned: bool,
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
    /// Optional keyboard shortcut label displayed next to the trigger name in
    /// the panel (e.g. `"⌘⇧T"`).  Display-only — not used for key registration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    /// When `true` the trigger appears in the Quick Launch strip at the top of
    /// the Triggers panel.
    #[serde(default)]
    pub pinned: bool,
    /// Standard five-field cron expression controlling when this trigger fires
    /// automatically (e.g. `"0 9 * * 1-5"` = weekdays at 9:00 AM UTC).
    /// `None` means the trigger has no automatic schedule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron_schedule: Option<String>,
    /// When `true` and `cron_schedule` is `Some`, the scheduler arms a timer
    /// for this trigger.  Stored so users can disable a schedule without
    /// losing the expression.
    #[serde(default)]
    pub schedule_enabled: bool,
    /// Absolute path of the TOML file this trigger was loaded from.
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

// ── Run history ───────────────────────────────────────────────────────────────

/// The outcome of a single trigger execution.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerRunStatus {
    Success,
    Stopped,
    TimedOut,
}

/// How a trigger run was initiated.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerRunSource {
    Manual,
    Scheduled,
}

/// One entry in a trigger's run history.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TriggerRunRecord {
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub status: TriggerRunStatus,
    pub source: TriggerRunSource,
}

/// Persisted run history for a single trigger, capped at [`TriggerHistory::MAX_RECORDS`].
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TriggerHistory {
    #[serde(default)]
    pub records: Vec<TriggerRunRecord>,
}

impl TriggerHistory {
    pub const MAX_RECORDS: usize = 100;

    /// Append `record`, dropping the oldest entry when the cap is exceeded.
    pub fn push(&mut self, record: TriggerRunRecord) {
        self.records.push(record);
        if self.records.len() > Self::MAX_RECORDS {
            self.records.remove(0);
        }
    }
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
    /// The shell that was running in the terminal pane at save time.
    /// `None` means "use the system default shell" (same as before this field existed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_launch_data: Option<ShellLaunchData>,
    /// When `true`, this tab was an Ambient Agent (Cloud Oz) pane rather than
    /// a regular terminal.  Restored via `add_ambient_agent_tab`.
    #[serde(default)]
    pub is_ambient_agent: bool,
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

        fn is_ambient_agent(node: &PaneNodeSnapshot) -> bool {
            match node {
                PaneNodeSnapshot::Leaf(leaf) => {
                    matches!(leaf.contents, LeafContents::AmbientAgent(_))
                }
                PaneNodeSnapshot::Branch(branch) => branch
                    .children
                    .iter()
                    .any(|(_, child)| is_ambient_agent(child)),
            }
        }

        let tabs = ws
            .tabs
            .iter()
            .map(|tab| {
                let ambient = is_ambient_agent(&tab.root);
                let terminal = first_terminal(&tab.root);
                WorkspaceTabSnapshot {
                    custom_title: tab.custom_title.clone(),
                    cwd: terminal.and_then(|t| t.cwd.clone()),
                    commands: vec![],
                    shell_launch_data: terminal.and_then(|t| t.shell_launch_data.clone()),
                    is_ambient_agent: ambient,
                }
            })
            .collect();

        WorkspaceSnapshot {
            tabs,
            active_tab_index: ws.active_tab_index,
        }
    }
}
