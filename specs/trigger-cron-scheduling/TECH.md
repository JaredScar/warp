# Trigger Cron Scheduling — Tech Spec

Reference `PRODUCT.md` for user-visible behavior. This doc covers implementation only.

## Context

**Current trigger system:**
- `app/src/actions/model.rs` — `Trigger` struct (id, name, description, action_ids, targets, hotkey, pinned, source_path). No scheduling fields.
- `app/src/actions/storage.rs` — TOML read/write helpers for `~/.warp/triggers/*.toml`.
- `app/src/actions/runner.rs` — `TriggerRunner::build_queue` decomposes a `Trigger` into a `VecDeque<TriggerQueueItem>` and hands it to `Workspace`.
- `app/src/workspace/view.rs:20747` — `RunTrigger(id)` arm in `Workspace::handle_action` is the single dispatch point; `advance_trigger_queue` (line 1379) steps through it, using `Timer::after` for command timeouts.
- `app/src/workspace/view.rs:1046–1060` — live trigger state: `trigger_queue`, `trigger_running_name`, `trigger_queue_waiting`, `trigger_active_terminal(_id)`.
- `app/src/actions/panel.rs` — `ActionsPanelView` (~2760 lines), `PanelMode` enum, `ActionsPanelTab`.

**Relevant patterns:**
- Timer: `ctx.spawn(async { warpui::r#async::Timer::after(duration).await; }, callback)` — used at `workspace/view.rs:1444`.
- App startup async work: `ctx.spawn(future, callback)` pattern used throughout `workspace/view.rs`.
- `ScheduledAmbientAgent` (`app/src/ai/ambient_agents/scheduled.rs:35`) has `cron_schedule: String` + `enabled: bool` — mirror this shape in `Trigger`.
- `chrono` is already a workspace dependency.

**New dependency needed:** `cron` crate (by zslayton) for parsing and computing next-fire times. The cron crate uses 7-field format (`sec min hour dom month dow year`); we expose 5-field to users (`min hour dom month dow`) and adapt by prepending `"0 "` (lock seconds to 0) and appending `" *"` (any year) when parsing.

## Proposed Changes

### 1. `app/Cargo.toml` — add cron crate

```toml
cron = "0.12"
```

And add to the root `Cargo.toml` workspace dependencies:

```toml
cron = "0.12"
```

### 2. `app/src/actions/model.rs` — extend `Trigger`

Add two fields to the `Trigger` struct:

```rust
/// Standard five-field cron expression controlling when this trigger fires
/// automatically (e.g. `"0 9 * * 1-5"` = weekdays at 9:00 AM UTC).
/// `None` means the trigger has no automatic schedule.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub cron_schedule: Option<String>,

/// When `true` and `cron_schedule` is `Some`, the scheduler arms a timer
/// for this trigger.  Stored so users can disable a schedule without losing
/// the expression.
#[serde(default)]
pub schedule_enabled: bool,
```

Add a new model type for history records:

```rust
/// The outcome of a single trigger execution.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerRunStatus {
    Success,
    Stopped,
    TimedOut,
}

/// How this trigger run was started.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerRunSource {
    Manual,
    Scheduled,
}

/// One entry in a trigger's run history.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TriggerRunRecord {
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: chrono::DateTime<chrono::Utc>,
    pub status: TriggerRunStatus,
    pub source: TriggerRunSource,
}

/// Persisted run history for a single trigger (capped at 100 records).
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TriggerHistory {
    #[serde(default)]
    pub records: Vec<TriggerRunRecord>,
}

impl TriggerHistory {
    pub const MAX_RECORDS: usize = 100;

    /// Append a record, dropping the oldest when over capacity.
    pub fn push(&mut self, record: TriggerRunRecord) {
        self.records.push(record);
        if self.records.len() > Self::MAX_RECORDS {
            self.records.remove(0);
        }
    }
}
```

### 3. `app/src/actions/storage.rs` — history persistence

Add a `trigger_history_dir()` helper (`~/.warp/trigger_history/`) and two functions:

```rust
pub fn load_trigger_history(trigger_id: Uuid) -> TriggerHistory { … }
pub fn save_trigger_history(trigger_id: Uuid, history: &TriggerHistory) -> Result<()> { … }
```

Each trigger's history is stored at `~/.warp/trigger_history/<trigger_id>.toml` as a TOML array of `TriggerRunRecord` wrapped in `TriggerHistory`. Loading a missing file returns an empty `TriggerHistory`.

### 4. `app/src/actions/scheduler.rs` — new module

A lightweight `CronScheduler` entity (or plain functions). Responsibilities:
- Parse a 5-field cron expression into a `cron::Schedule` (by prepending `"0 "` and appending `" *"`).
- Compute `duration_until_next`: given `Schedule` and `Utc::now()`, call `schedule.upcoming(Utc).next()` → `DateTime<Utc>`, subtract now, clamp to `Duration::from_secs(1)` minimum.
- `start_trigger_schedule(trigger_id, cron_expr, workspace_handle, ctx)` — spawns a `ctx.spawn` async loop:
  ```
  loop {
      compute duration_until_next from expression
      Timer::after(duration).await
      dispatch WorkspaceAction::RunTrigger(trigger_id) on workspace_handle
  }
  ```
  Returns an abort handle stored in the scheduler state.
- `stop_trigger_schedule(trigger_id)` — cancels the abort handle.
- `reload_all(triggers, workspace_handle, ctx)` — called at startup and whenever the trigger list changes (new trigger saved, trigger deleted, schedule toggled). Diffs current abort handles against the new trigger list, cancelling stale timers and starting new ones.

The scheduler state (a `HashMap<Uuid, AbortHandle>`) lives on `ActionsPanelView` (the natural owner of trigger state) or as a `Model` entity. The simplest ownership: a new `CronScheduler` struct that `ActionsPanelView` holds by value.

**Cron dispatch and workspace handle:** `ActionsPanelView` already dispatches `WorkspaceAction::RunTrigger(id)` via `ctx.dispatch_typed_action`. The scheduler can use the same approach — the workspace handle is not needed directly; `ctx.dispatch_typed_action` broadcasts to the active workspace.

**Skip if already running:** In `Workspace::handle_action`'s `RunTrigger` arm, if `self.trigger_running_name.is_some()`, return early (skip). Add the source annotation (manual vs. scheduled) by routing through a new `RunTriggerFromSchedule(id)` variant or by adding a boolean argument. Prefer a new `WorkspaceAction` variant to keep the distinction clean:

```rust
RunTrigger { id: Uuid, source: TriggerRunSource },
// existing callers become: RunTrigger { id, source: TriggerRunSource::Manual }
// scheduler dispatches:     RunTrigger { id, source: TriggerRunSource::Scheduled }
```

### 5. `app/src/workspace/view.rs` — run recording + skip-if-busy

In the `RunTrigger` arm:

1. If `self.trigger_running_name.is_some()` → return early, skip.
2. Store `trigger_start_time: Option<(DateTime<Utc>, TriggerRunSource)>` on `Workspace`.
3. Call `TriggerRunner::build_queue`.
4. In `advance_trigger_queue` where the queue drains to empty (line 1388–1394), if `trigger_start_time.is_some()` write the history record with `status = Success`.
5. In `StopTrigger`, if `trigger_start_time.is_some()` write the history record with `status = Stopped`.
6. For `TimedOut`: the timeout path (line 1363) can set a flag; when the queue is subsequently drained, check the flag to emit `TimedOut`.

History writes: `storage::save_trigger_history(trigger_id, &updated_history)` — load existing history, push the new record, save. Do this inline (on main thread) since it's a small TOML write; no need for async.

### 6. `app/src/actions/panel.rs` — UI changes

**`PanelMode` enum** — add:

```rust
ViewHistory(Uuid), // trigger id whose history is shown
```

**Trigger editor form** — below the existing fields, add a `Schedule` section:
- Text field bound to a `draft_cron_schedule: String` field on the panel.
- Parsed and validated on every keystroke; show a plain-English preview on success or an inline error on failure. Use `cron_to_human_readable(expr)` — a small helper using `cron::Schedule` to format the next few occurrences into a description.
- An enable/disable toggle bound to `draft_schedule_enabled: bool`.

**Trigger list row** — render a clock icon button on rows where the trigger has an active schedule. Hovering shows the next-fire time tooltip. A separate history icon button opens `PanelMode::ViewHistory(trigger.id)`.

**History panel** — new `render_history_panel(trigger_id, ctx)` function:
- Header with `← Back` button (sets `PanelMode::List`).
- List of `TriggerRunRecord` from `storage::load_trigger_history(trigger_id)`.
- Empty-state placeholder when no records.

**Cron scheduler wiring:** after saving a trigger (new or updated), call `self.cron_scheduler.reload_all(&config.triggers(), ctx)` where `self.cron_scheduler` is the `CronScheduler` held on the panel.

**Panel init:** call `reload_all` once from `ActionsPanelView::new` with the current trigger list.

## Diagram

```
User saves trigger (cron_expr set, enabled)
         │
         ▼
ActionsPanelView::save_trigger()
  └─► write_trigger() → ~/.warp/triggers/<id>.toml
  └─► CronScheduler::reload_all()
          │
          ▼
    start_trigger_schedule(id, expr)
          │
    ctx.spawn(async loop)
          │
          ▼   (timer fires at next cron occurrence)
    dispatch WorkspaceAction::RunTrigger { id, source: Scheduled }
          │
          ▼
    Workspace::handle_action(RunTrigger)
      skip if trigger_running_name.is_some()
      set trigger_start_time = (Utc::now(), Scheduled)
      TriggerRunner::build_queue(...)
          │
          ▼   (queue drains)
    advance_trigger_queue (empty branch)
      write TriggerRunRecord { status: Success, ... }
      save_trigger_history(id, &history)
```

## Testing and Validation

- **Behavior 1–5 (cron config):** Unit tests in `scheduler.rs` that parse valid and invalid 5-field expressions, verify the 7-field adapter, and verify `duration_until_next` never returns a negative duration.
- **Behavior 9 (auto-exec):** Integration test that creates a trigger with a `* * * * *` schedule (every minute, but use a 1-second override in tests), advances the timer mock, and asserts `WorkspaceAction::RunTrigger` was dispatched.
- **Behavior 11–12 (skip if busy):** Unit test on `Workspace::handle_action`: set `trigger_running_name = Some(...)`, dispatch `RunTrigger`, assert the queue remains empty.
- **Behavior 15–16 (history):** Unit test on `TriggerHistory::push` at 100 + 1 records, assert len stays ≤ 100 and the oldest is dropped.
- **Behavior 20 (persistence):** Round-trip test: save a `TriggerRunRecord` via `save_trigger_history`, reload with `load_trigger_history`, compare.
- **Manual:** Open the editor, type a valid cron string, verify the preview text. Type an invalid string, verify the Save button is disabled. Save, re-open, verify the expression is pre-populated.

## Risks and Mitigations

- **`cron` crate version lock:** The `cron` crate is a small, stable dependency. If it disappears we can vendor it or switch to `croner`. Risk is low.
- **5→7 field adapter bugs:** Validated by unit tests at the parsing layer.
- **History file corruption:** `load_trigger_history` returns an empty `TriggerHistory` on any parse error, so corruption is non-fatal.
- **Timer drift:** `warpui::r#async::Timer` is not a cron-aware scheduler — each occurrence recomputes `Utc::now()` + next-occurrence, so drift is bounded to one event loop tick (~ms), not accumulating.
