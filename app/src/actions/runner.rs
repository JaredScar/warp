use warpui::{AppContext, ViewContext, ViewHandle};

use crate::pane_group::PaneGroup;
use crate::workspace::Workspace;

use super::model::{Action, Trigger, TriggerTargets};

/// Resolves the terminal panes targeted by a trigger and sends each action's
/// commands to all of them, executing actions in sequence.
///
/// Within each action all resolved terminal panes receive the commands
/// concurrently (dispatched in a single pass), so multiple terminals progress
/// in lock-step.  The trigger advances to the next action only after the
/// current action has been sent to all targets.
pub struct TriggerRunner;

impl TriggerRunner {
    /// Execute `trigger` using the actions found in `all_actions`.
    ///
    /// No-ops gracefully if the trigger references unknown action IDs or if no
    /// terminal targets can be resolved.
    pub fn run(
        trigger: &Trigger,
        all_actions: &[Action],
        workspace: &ViewHandle<Workspace>,
        ctx: &mut AppContext,
    ) {
        let target_groups: Vec<ViewHandle<PaneGroup>> =
            workspace.read(ctx, |ws, app| Self::resolve_targets(ws, &trigger.targets, app));

        if target_groups.is_empty() {
            return;
        }

        for action_id in &trigger.action_ids {
            let Some(action) = all_actions.iter().find(|a| &a.id == action_id) else {
                continue;
            };
            Self::dispatch_action(action, &target_groups, ctx);
        }
    }

    fn resolve_targets(
        ws: &Workspace,
        targets: &TriggerTargets,
        ctx: &AppContext,
    ) -> Vec<ViewHandle<PaneGroup>> {
        let count = ws.tab_count();
        let all: Vec<ViewHandle<PaneGroup>> = (0..count)
            .filter_map(|i| ws.get_pane_group_view(i).cloned())
            .collect();

        match targets {
            TriggerTargets::AllOpen => all,
            TriggerTargets::ByIndex { indices } => all
                .into_iter()
                .enumerate()
                .filter_map(|(i, g)| if indices.contains(&i) { Some(g) } else { None })
                .collect(),
            TriggerTargets::ByTitle { titles } => all
                .into_iter()
                .filter(|g| {
                    let title = g
                        .as_ref(ctx)
                        .custom_title(ctx)
                        .unwrap_or_default()
                        .to_lowercase();
                    titles.iter().any(|t| title.contains(&t.to_lowercase()))
                })
                .collect(),
        }
    }

    /// Send every command in `action` to every terminal pane in every target
    /// group.  All dispatches happen synchronously so they are effectively
    /// concurrent from the shell's perspective.
    fn dispatch_action(
        action: &Action,
        groups: &[ViewHandle<PaneGroup>],
        ctx: &mut AppContext,
    ) {
        for group_handle in groups {
            group_handle.update(ctx, |group, group_ctx| {
                Self::dispatch_action_to_group(action, group, group_ctx);
            });
        }
    }

    fn dispatch_action_to_group(
        action: &Action,
        group: &mut PaneGroup,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        if action.commands.is_empty() {
            return;
        }
        // Join all commands with newlines so they are sent as a single multi-line
        // input rather than appended one-by-one to the live input buffer.  Shells
        // with bracketed paste preserve the '\n' separators and execute each line
        // in sequence; shells without bracketed paste receive '\r'-separated lines
        // which are each executed as individual commands.
        let combined = action.commands.join("\n");
        for terminal_handle in group.terminal_views(ctx) {
            terminal_handle.update(ctx, |terminal, term_ctx| {
                terminal.execute_command_or_set_pending(&combined, term_ctx);
            });
        }
    }
}
