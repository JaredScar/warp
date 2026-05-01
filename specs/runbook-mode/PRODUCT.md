# Runbook Mode

## Summary

Runbook Mode lets users create named, step-by-step runbooks — ordered lists of named shell commands — and execute them in the active terminal individually or all at once. Each step tracks its own run status (not run / running / passed / failed), and sequential "Run All" halts automatically on the first failure.

## Behavior

### Runbook list

1. The Actions & Triggers panel gains a fifth tab, **"Runbooks"**, displayed after "Rules".
2. The Runbooks tab shows a flat list of all saved runbooks, sorted by name.
3. Each row shows the runbook name and the step count (e.g. "5 steps").
4. When no runbooks exist, an empty state is shown: title "Runbooks", subtitle "Document and run step-by-step procedures", and a "+ Create Runbook" button.
5. A **+** icon button in the tab toolbar always creates a new runbook regardless of whether the list is empty.

### Creating and editing a runbook

6. Clicking "+ Create Runbook" or the toolbar **+** button opens the runbook editor form inline, replacing the list.
7. The editor has:
   - A **Name** field (single-line, required). Placeholder: "e.g. Deploy to staging".
   - An ordered list of **steps**, each with:
     - A **Step name** field (single-line, required). Placeholder: "e.g. Build the project".
     - A **Command** field (single-line, required). Placeholder: "e.g. cargo build --release".
   - An **"+ Add Step"** button below the step list.
   - **Save** and **Cancel** buttons at the bottom.
8. Steps are ordered; drag handles are not required for MVP — steps are added in sequence and can be deleted.
9. Each step row has a **delete (×)** button that removes it immediately.
10. The form cannot be saved if the name is empty or if any step has an empty name or empty command. The Save button is disabled (visually muted) in those cases.
11. Clicking **Save** persists the runbook and returns to the list view with the new runbook visible.
12. Clicking **Cancel** discards changes and returns to the list view.
13. Editing an existing runbook opens the same form pre-populated with that runbook's current name and steps.

### Running a runbook

14. Each runbook row in the list has a **▶ Run All** button and an **Edit (pencil)** button.
15. Clicking **▶ Run All** transitions the panel to the **runner view** for that runbook (see below).
16. The runner view shows the runbook name as a header, a **← Back** button, and the ordered list of steps.
17. Each step row in the runner view shows:
    - The step name.
    - The command (dimmed, monospace).
    - A status indicator: **—** (not run), **⟳** (running), **✓** (passed), **✗** (failed).
    - A **▶ Run** button to execute that step in isolation.
18. The **▶ Run All** button in the runner view header starts sequential execution from the first not-run step (or restarts from the top if all steps are already complete or failed).
19. Sequential execution sends each step's command to the active terminal via `WorkspaceAction::RunActionInActiveTerminal`-equivalent dispatch, waits for the command to finish, then:
    - If the exit code is **0**, marks the step ✓ and continues to the next step.
    - If the exit code is **non-zero**, marks the step ✗ and **halts** — no further steps run automatically.
20. While a step is running (⟳), the **▶ Run All** button is disabled and individual **▶ Run** buttons for all other steps are also disabled.
21. Clicking an individual **▶ Run** button runs only that step; it does not affect other steps' statuses.
22. After any run (individual or sequential), the step's status updates to ✓ or ✗ based on exit code.
23. A **Reset** button in the runner view header resets all step statuses back to — (not run) without running anything.
24. Status indicators are purely in-memory and not persisted to disk — reopening the panel or switching tabs resets all step statuses.

### Deleting a runbook

25. The edit form has a **Delete** button (destructive, styled differently) at the bottom.
26. Clicking Delete immediately removes the runbook from the list and from disk, and returns to the list view. No confirmation dialog is required for MVP.

### Persistence

27. Runbooks are stored in `~/.warp/runbooks/` as TOML files, one file per runbook, named `<uuid>.toml`.
28. Each file contains the runbook name, a UUID, and an ordered list of steps (each with a UUID, name, and command).
29. Runbooks load asynchronously on startup alongside actions and triggers; missing or malformed files are skipped with a warning, not a crash.
30. Changes (save, delete) are written to disk synchronously before the UI updates.

### Edge cases

31. If the active terminal has no running shell when **▶ Run** or **▶ Run All** is clicked, the command is dispatched the same way as a normal action — no special error is shown in the panel.
32. If the user switches away from the Runbooks tab while a run is in progress, execution continues in the background; step statuses update when the user returns to the tab.
33. If the user closes and reopens the Actions panel while a run is in progress, the in-progress state is lost (statuses reset to —). MVP does not persist run state.
34. Runbook names are not required to be unique; two runbooks may share the same name.
35. A runbook with zero steps can be saved but the **▶ Run All** button is disabled (nothing to run).
