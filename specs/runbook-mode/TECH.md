# Runbook Mode — Tech Spec

## Context

This spec implements the Runbook Mode feature described in `PRODUCT.md`. Runbooks are ordered, named lists of shell-command steps stored on disk and executable from the Actions & Triggers panel.

The existing Actions panel (`app/src/actions/panel.rs`) already has a pluggable tab system (Actions, Triggers, Workspaces, Rules). Runbooks follow the same patterns used by the Tab Naming Rules tab added in commit `cb9e57b`: a new `ActionsPanelTab` variant, matching render and action-handler arms, and new model types with TOML persistence.

Sequential execution reuses the `trigger_queue` / `advance_trigger_queue` infrastructure in `app/src/workspace/view.rs` (`Workspace`, lines ~1045–1510) via `TriggerQueueItem::SendCommand`. Step status is tracked as in-memory `RunbookRunState` on the panel view.

Relevant files:
- `app/src/actions/model.rs` — Action/Trigger model types; new `Runbook`/`RunbookStep` go here.
- `app/src/actions/storage.rs` — Per-file TOML save/delete/load helpers; add `runbooks_dir`, `save_runbook`, `delete_runbook`, `load_runbooks`.
- `app/src/actions/mod.rs` — Expose `runbook` submodule.
- `app/src/user_config/mod.rs` — `WarpConfig` model; add `runbooks: Vec<Runbook>` field and `WarpConfigUpdateEvent::Runbooks` variant.
- `app/src/user_config/native.rs` — Async startup load and file-watcher for `~/.warp/runbooks/`; add `load_runbooks` alongside `load_naming_rules`.
- `app/src/actions/panel.rs` — All UI: new tab variant, view state, render functions, and action handlers.
- `app/src/workspace/action.rs` — Add `RunCommandInActiveTerminal(String)` workspace action.
- `app/src/workspace/view.rs` — Handle `RunCommandInActiveTerminal`.

## Proposed Changes

### 1. Model (`app/src/actions/model.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookStep {
    pub id: Uuid,
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runbook {
    pub id: Uuid,
    pub name: String,
    pub steps: Vec<RunbookStep>,
}
```

### 2. Storage (`app/src/actions/storage.rs`)

Add:
- `pub fn runbooks_dir() -> PathBuf` → `data_dir().join("runbooks")`
- `pub fn save_runbook(runbook: &Runbook) -> Result<()>` — writes `<id>.toml`
- `pub fn delete_runbook(id: Uuid) -> Result<()>` — removes `<id>.toml`
- `pub fn load_runbooks(dir: &Path) -> Vec<Runbook>` — reads all `.toml` files, skips malformed

Also export `runbooks_dir` from `app/src/user_config/mod.rs` alongside the existing path helpers.

### 3. WarpConfig (`app/src/user_config/mod.rs` + `native.rs`)

- Add `runbooks: Vec<Runbook>` field to `WarpConfig` (initialized empty).
- Add `WarpConfigUpdateEvent::Runbooks` variant.
- Add `pub fn runbooks(&self) -> &[Runbook]`, `add_runbook`, `update_runbook`, `remove_runbook` methods (mirroring the naming-rules pattern).
- In `native.rs` `new()` async spawn: load `runbooks` from `runbooks_dir()` alongside actions/triggers.
- Add file-watcher branch for `runbooks_dir()` so the list refreshes if files change on disk.

### 4. Workspace action (`app/src/workspace/action.rs`)

Add one new variant:
```rust
RunCommandInActiveTerminal(String),
```
This replaces looking up by `Uuid` — runbook steps carry their command directly. The handler in `view.rs` mirrors `RunActionInActiveTerminal` but accepts the string directly.

### 5. Workspace handler (`app/src/workspace/view.rs`)

```rust
RunCommandInActiveTerminal(cmd) => {
    if let Some(terminal_handle) = self
        .active_tab_pane_group()
        .read(ctx, |pg, app| pg.active_session_view(app))
    {
        terminal_handle.update(ctx, |terminal, term_ctx| {
            terminal.execute_command_or_set_pending(cmd, term_ctx);
        });
    }
}
```

Sequential "Run All" uses `TriggerQueueItem::SendCommand` items pushed onto `self.trigger_queue`, with `trigger_running_name` set to the runbook name (shows the running indicator). Each step uses a short default timeout. Step status in the panel is updated via a new `ActionsPanelAction::SetRunbookStepStatus { runbook_id, step_id, status }` dispatched from the panel when execution starts and from a workspace subscription when `PendingCommandCompleted` fires.

**Simpler MVP approach for step status:** Because wiring exit codes requires deeper terminal event integration, the MVP marks a step ✓ as soon as `PendingCommandCompleted` fires (command left the input box), rather than checking the exit code. Sequential execution advances unconditionally. Exit-code-based pass/fail is deferred to a follow-up.

### 6. Panel (`app/src/actions/panel.rs`)

**Struct fields added:**

```rust
// ── Runbook tab ──────────────────────────────────────────────────────────────
runbooks_tab_mouse_state: MouseStateHandle,
new_runbook_mouse_state: MouseStateHandle,
runbook_row_states: RefCell<HashMap<Uuid, RowMouseStates>>,
// ── Runbook editor form ──────────────────────────────────────────────────────
edit_runbook_id: Option<Uuid>,           // None = new
edit_runbook_name_editor: ViewHandle<EditorView>,
runbook_step_editors: Vec<(Uuid, ViewHandle<EditorView>, ViewHandle<EditorView>)>, // (step_id, name_editor, cmd_editor)
runbook_form_open: bool,
// ── Runbook runner ───────────────────────────────────────────────────────────
running_runbook_id: Option<Uuid>,        // Some = runner view is open
runbook_step_statuses: HashMap<Uuid, StepStatus>,  // step_id → status
runbook_run_all_button_state: MouseStateHandle,
runbook_reset_button_state: MouseStateHandle,
runbook_back_button_state: MouseStateHandle,
```

**New enum:**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus { NotRun, Running, Done }
```

**New `ActionsPanelTab` variant:** `Runbooks`

**New `ActionsPanelAction` variants:**
```rust
// List view
NewRunbook,
EditRunbook(Uuid),
DeleteRunbook(Uuid),
SaveRunbook,
// Runner view
OpenRunner(Uuid),
CloseRunner,
RunStep(Uuid),          // step_id within the open runner's runbook
RunAllSteps,
ResetSteps,
SetStepStatus { step_id: Uuid, status: StepStatus },
// Step editor management
AddRunbookStep,
RemoveRunbookStep(Uuid),
```

**New render functions:**
- `render_runbooks_tab` — list view with toolbar + rows
- `render_runbook_row` — single row with name, step count, Run All / Edit buttons
- `render_runbook_editor` — inline form with name field and step list
- `render_runner_view` — step list with status icons and Run buttons

### 7. `ActionsPanelTab` match arm (`render_header` and `render` body)

Follow the exact same pattern as `NamingRules`: add `Runbooks =>` arms in the tab-button row, the header left-side match, and the content match. No conditional logic — the tab is always visible.

## Testing and validation

- **Behavior 1–5 (list & empty state):** Open the Actions panel, navigate to "Runbooks" — verify the tab is visible and empty state shows.
- **Behavior 6–13 (editor):** Create a runbook with 2 steps, save, confirm it appears in the list and in `~/.warp/runbooks/`.
- **Behavior 14–17 (runner view):** Click ▶ Run All on a runbook, confirm each step's command appears in the active terminal.
- **Behavior 18–22 (sequential):** Verify steps execute in order with the ⟳ → ✓ transition.
- **Behavior 23 (reset):** Click Reset, confirm all statuses return to —.
- **Behavior 25–26 (delete):** Delete a runbook, confirm it's removed from disk.
- **Behavior 29–30 (persistence):** Restart the app, confirm saved runbooks reload.
- **Behavior 35 (zero steps):** Save a runbook with no steps, confirm Run All is disabled.

## Follow-ups

- Exit-code-based pass/fail gates (requires terminal exit-code event plumbing).
- Step reordering via drag handles.
- Runbook import/export.
