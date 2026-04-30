# Trigger Cron Scheduling

## Summary

Users can attach a cron schedule to any trigger so it fires automatically at the configured time, without manual interaction. Each trigger also accumulates a run history so users can see when it last ran, how long it took, and whether it succeeded.

## Behavior

### Cron Schedule Configuration

1. Each trigger has an optional cron schedule expressed as a standard five-field cron string (`minute hour day-of-month month day-of-week`, e.g. `0 9 * * 1-5` for weekdays at 9 AM). When no schedule is set the trigger behaves exactly as today — it can only be run manually.

2. When editing a trigger, the editor form gains a **Schedule** section below the existing fields containing:
   - A text field labelled **Cron expression** that accepts any five-field cron string.
   - An **Enable schedule** toggle that is `off` by default for new triggers and `on` for any trigger that was previously saved with a schedule.
   - When the expression field is empty the toggle is disabled and cannot be turned on.

3. The cron expression field validates the input on every keystroke:
   - If the expression is syntactically valid, the field shows no error and a human-readable "plain English" preview appears below it (e.g. `Every weekday at 9:00 AM`).
   - If the expression is syntactically invalid, the field shows an inline error message and the **Save** button is disabled.
   - An empty expression is not an error — it simply means no schedule.

4. Saving a trigger with a non-empty, valid expression and the toggle enabled persists both the expression and the enabled state. Saving with the toggle off but a non-empty expression persists the expression (so it can be re-enabled later) but does not schedule the trigger to run.

5. Clearing the expression field and saving removes the schedule entirely.

### Trigger List Display

6. In the Triggers list, any trigger with an active schedule (expression set, enabled) shows a small clock icon on its row alongside the other action icons.

7. Hovering the clock icon (or the row when no other hover target is active) shows a tooltip with: **Next run: `<day>, <date> at <time>`** formatted in the user's local timezone. If the next fire time cannot be computed (e.g. the expression is valid but produces no future occurrence), the tooltip reads **Schedule has no future occurrences**.

8. Triggers with a disabled schedule (expression saved but toggle off) show no clock icon and no tooltip — they appear identical to unscheduled triggers in the list.

### Automatic Execution

9. When the app starts and a trigger has an active schedule, the scheduler computes the time until the next cron occurrence and arms a one-shot timer. When the timer fires it runs the trigger exactly as if the user had clicked the play button, then immediately arms a new timer for the following occurrence.

10. If the app is not running when a cron occurrence passes, that occurrence is simply skipped — Warp does not back-fill missed runs.

11. A scheduled trigger that is already running (the trigger-running overlay is active) skips the cron occurrence that fires while it is still running. The next occurrence after the run finishes is still armed normally.

12. If another trigger is running when a scheduled trigger's timer fires, the scheduled trigger's occurrence is skipped (the same single-trigger-at-a-time constraint as manual runs). The next occurrence is still armed.

13. When a trigger's schedule is edited (expression changed, toggle flipped, trigger deleted), the previously armed timer is cancelled before the new timer is set. There is no window where two timers for the same trigger can be active simultaneously.

14. When the app moves to the background or the screen locks, in-progress triggers continue running. Timers continue to arm normally — the OS will wake the timer when the app is next active if the app was suspended.

### Run History

15. Every time a trigger finishes running — whether fired manually or by the cron scheduler — a run record is appended to that trigger's history. The record contains:
    - **Started at**: wall-clock timestamp when the trigger began executing (UTC, stored as ISO-8601).
    - **Finished at**: wall-clock timestamp when execution ended (all commands completed, the trigger was stopped by the user, or the last command timed out).
    - **Duration**: derived from started/finished times; displayed in the UI as `Xs` or `Xm Ys`.
    - **Status**: one of `success`, `stopped` (user clicked Stop Trigger), or `timed_out` (last command exceeded its timeout).
    - **Source**: `manual` or `scheduled`.

16. History is capped at the 100 most recent records per trigger. When a new record would push the count past 100, the oldest record is dropped.

17. Each trigger row in the list has a **History** button (clock-with-list icon) that opens a history panel inline within the Actions & Triggers panel, replacing the list. The history panel shows:
    - A header: **`<trigger name>` — Run History** with a **← Back** button.
    - A chronological list (newest first) of run records. Each record shows:
      - A status icon: green checkmark (`success`), orange stop sign (`stopped`), red clock (`timed_out`).
      - The started-at time, formatted as a relative time when recent (e.g. `2 hours ago`) and as an absolute date/time when older than 24 hours.
      - The duration.
      - A `scheduled` or `manual` badge.
    - When the trigger has no history yet, the panel shows an empty state: **No runs yet. Run this trigger to see history here.**

18. The history panel is read-only. The user can return to the trigger list with the **← Back** button or by switching tabs.

19. History persists across app restarts. It is stored locally on disk alongside the trigger TOML files and is not synced to the cloud.

20. If the TOML file backing a trigger is deleted externally (e.g. the user removes it from `~/.warp/triggers/`), its history file is not automatically deleted — it becomes orphaned. Orphaned history files do not affect app behaviour.

### Edge Cases

21. If a trigger referenced by an active cron schedule has had all its actions deleted (so the action list is empty), the scheduler silently skips the occurrence and logs a warning — it does not crash or show an error to the user.

22. If the system clock is moved backward while the app is running, the next timer arm uses the adjusted time. If this causes a timer to fire "early" (the adjusted fire time is in the past), it fires immediately and arms the subsequent occurrence normally.

23. Importing a trigger TOML file with a `cron_schedule` field (e.g. copied from another machine) picks up the schedule automatically on the next app launch. The `schedule_enabled` field defaults to `true` if absent from an imported file that has a non-empty expression, preserving backwards compatibility.

24. The cron expression uses UTC as the reference timezone for computation; displayed times in the UI are shown in the user's local timezone.

## Figma

Figma: none provided
