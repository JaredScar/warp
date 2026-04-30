//! Cron-based automatic scheduling for [`Trigger`]s.
//!
//! [`CronScheduler`] owns one abort-able timer per active trigger schedule.
//! Call [`CronScheduler::reload_all`] whenever the trigger list changes (on
//! startup, after save, after delete).  Each timer fires at the next cron
//! occurrence, dispatches [`WorkspaceAction::RunTriggerScheduled`], then
//! immediately re-arms itself for the following occurrence.

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use chrono::{Local, Utc};
use cron::Schedule;
use uuid::Uuid;
use warpui::{r#async::SpawnedFutureHandle, ViewContext};

use super::model::Trigger;
use crate::actions::panel::ActionsPanelView;
use crate::workspace::WorkspaceAction;

// ── Public API ────────────────────────────────────────────────────────────────

/// Manages per-trigger cron timers for automatic execution.
///
/// Each active scheduled trigger has one [`SpawnedFutureHandle`] stored here.
/// Aborting the handle cancels the pending timer and prevents any future
/// dispatches for that trigger until a new handle is armed.
pub struct CronScheduler {
    /// Maps trigger ID → abort handle for its currently-armed cron timer.
    entries: HashMap<Uuid, SpawnedFutureHandle>,
}

impl CronScheduler {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Parse a standard five-field cron expression (`min hour dom month dow`)
    /// into a [`Schedule`].  Returns `None` for syntactically invalid input.
    ///
    /// The `cron` crate uses a seven-field format
    /// (`sec min hour dom month dow year`).  We adapt by prepending `"0 "`
    /// (lock seconds to zero) and appending `" *"` (any year).
    pub fn parse_expression(expr: &str) -> Option<Schedule> {
        let adapted = format!("0 {} *", expr.trim());
        Schedule::from_str(&adapted).ok()
    }

    /// Return the [`Duration`] until the next occurrence of `schedule` after
    /// the current wall-clock time.
    ///
    /// Returns `None` when the schedule has no future occurrences.
    /// The returned duration is clamped to a minimum of 1 second so that we
    /// never arm a zero-duration (immediate) timer when the clock is exactly
    /// on a boundary.
    pub fn duration_until_next(schedule: &Schedule) -> Option<Duration> {
        let next = schedule.upcoming(Utc).next()?;
        let now = Utc::now();
        let delta = next.signed_duration_since(now);
        let secs = delta.num_seconds().max(1) as u64;
        Some(Duration::from_secs(secs))
    }

    /// Return the next fire [`chrono::DateTime`] in UTC, or `None` when the
    /// expression has no future occurrences.
    pub fn next_fire_time_utc(expr: &str) -> Option<chrono::DateTime<Utc>> {
        let schedule = Self::parse_expression(expr)?;
        schedule.upcoming(Utc).next()
    }

    /// Return a short human-readable description of the schedule such as
    /// `"Next: Mon May 4 at 9:00 AM"`, suitable for a tooltip.
    ///
    /// Returns `None` when the expression is invalid or has no future
    /// occurrences.
    pub fn next_fire_label(expr: &str) -> Option<String> {
        let next_utc = Self::next_fire_time_utc(expr)?;
        let next_local: chrono::DateTime<Local> = next_utc.into();
        Some(format!(
            "Next: {}",
            next_local.format("%a %b %-d at %-I:%M %p")
        ))
    }

    /// Validate `expr` and return a plain-English preview string on success,
    /// or an error message string on failure.
    pub fn validate_and_describe(expr: &str) -> Result<String, String> {
        if expr.trim().is_empty() {
            return Err("Enter a cron expression (e.g. 0 9 * * 1-5)".to_string());
        }
        match Self::parse_expression(expr) {
            None => Err("Invalid cron expression".to_string()),
            Some(_) => Ok(Self::next_fire_label(expr)
                .unwrap_or_else(|| "No future occurrences".to_string())),
        }
    }

    // ── Timer management ──────────────────────────────────────────────────

    /// Diff `triggers` against the currently-armed entries, cancelling stale
    /// timers and arming new ones.  Call after every trigger-list mutation.
    pub fn reload_all(
        &mut self,
        triggers: &[Trigger],
        ctx: &mut ViewContext<ActionsPanelView>,
    ) {
        // Build the set of (id, expr) pairs that should be active.
        let desired: HashMap<Uuid, String> = triggers
            .iter()
            .filter_map(|t| {
                if !t.schedule_enabled {
                    return None;
                }
                let expr = t.cron_schedule.as_deref()?.trim().to_string();
                if expr.is_empty() {
                    return None;
                }
                // Validate; skip if unparseable.
                Self::parse_expression(&expr)?;
                Some((t.id, expr))
            })
            .collect();

        // Cancel handles that are no longer needed.
        self.entries.retain(|id, handle: &mut SpawnedFutureHandle| {
            if !desired.contains_key(id) {
                handle.abort();
                false
            } else {
                true
            }
        });

        // Arm timers for newly-scheduled triggers.
        for (id, expr) in &desired {
            if !self.entries.contains_key(id) {
                if let Some(handle) = Self::arm_new(*id, expr.clone(), ctx) {
                    self.entries.insert(*id, handle);
                }
            }
        }
    }

    /// Cancel the timer for a single trigger (e.g. when it is deleted).
    pub fn cancel(&mut self, trigger_id: Uuid) {
        if let Some(handle) = self.entries.remove(&trigger_id) {
            handle.abort();
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────

    /// Arm a single timer for `trigger_id` using `expr`.
    ///
    /// On fire the timer:
    /// 1. Dispatches [`WorkspaceAction::RunTriggerScheduled`].
    /// 2. Re-arms itself for the next occurrence (if the entry still exists —
    ///    i.e. the schedule wasn't cancelled while the timer was in flight).
    fn arm_new(
        trigger_id: Uuid,
        expr: String,
        ctx: &mut ViewContext<ActionsPanelView>,
    ) -> Option<SpawnedFutureHandle> {
        let schedule = Self::parse_expression(&expr)?;
        let duration = Self::duration_until_next(&schedule)?;

        let expr_for_callback = expr.clone();
        let handle = ctx.spawn_abortable(
            async move {
                warpui::r#async::Timer::after(duration).await;
            },
            move |me: &mut ActionsPanelView, _result, ctx| {
                // Dispatch the scheduled run.
                ctx.dispatch_typed_action(&WorkspaceAction::RunTriggerScheduled(trigger_id));

                // Re-arm for the next occurrence only if this trigger is still
                // registered in the scheduler (it could have been removed while
                // the timer was in-flight).
                if me.cron_scheduler.entries.contains_key(&trigger_id) {
                    if let Some(new_handle) =
                        Self::arm_new(trigger_id, expr_for_callback.clone(), ctx)
                    {
                        me.cron_scheduler
                            .entries
                            .insert(trigger_id, new_handle);
                    } else {
                        // No future occurrences — remove the entry.
                        me.cron_scheduler.entries.remove(&trigger_id);
                    }
                }
            },
            |_me: &mut ActionsPanelView, _ctx| {
                // Aborted — no-op; the entry is already removed by `cancel`.
            },
        );

        Some(handle)
    }
}
