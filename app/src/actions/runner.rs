use warpui::{AppContext, ViewContext, ViewHandle};

use crate::pane_group::PaneGroup;
use crate::workspace::Workspace;
use crate::WorkspaceAction;

use super::model::{
    is_builtin_action, Action, Trigger,
    BUILTIN_CLOSE_ALL_TERMINALS_ID, BUILTIN_KILL_ALL_PROCESSES_ID,
};

/// Executes the actions referenced by a trigger.
///
/// For each action in the trigger, a fresh terminal tab is opened and the
/// action's commands are dispatched to it (set as a pending command that
/// executes once the shell is ready).  This keeps trigger-run commands
/// isolated from whatever was already open.
///
/// Built-in system actions (`Close All Terminals`, `Kill All Terminal
/// Processes`) are handled via dedicated `WorkspaceAction` dispatches
/// instead of shell commands.
pub struct TriggerRunner;

impl TriggerRunner {
    pub fn run(
        trigger: &Trigger,
        all_actions: &[Action],
        workspace: &ViewHandle<Workspace>,
        ctx: &mut AppContext,
    ) {
        for action_id in &trigger.action_ids {
            let Some(action) = all_actions.iter().find(|a| &a.id == action_id) else {
                continue;
            };

            // Built-in actions are dispatched as WorkspaceActions rather than
            // sending shell commands.
            if is_builtin_action(action_id) {
                Self::dispatch_builtin(action_id, workspace, ctx);
                continue;
            }

            if action.commands.is_empty() {
                continue;
            }

            // Open a fresh terminal tab for this action, then dispatch the
            // joined commands to the newly created pane.
            workspace.update(ctx, |ws, ws_ctx| {
                ws.add_terminal_tab(true, ws_ctx);
            });

            let new_group = workspace.read(ctx, |ws, _| ws.active_tab_pane_group().clone());
            new_group.update(ctx, |group, group_ctx| {
                Self::dispatch_action_to_group(action, group, group_ctx);
            });
        }
    }

    fn dispatch_builtin(
        action_id: &uuid::Uuid,
        workspace: &ViewHandle<Workspace>,
        ctx: &mut AppContext,
    ) {
        if *action_id == BUILTIN_CLOSE_ALL_TERMINALS_ID {
            workspace.update(ctx, |_, ws_ctx| {
                ws_ctx.dispatch_typed_action(&WorkspaceAction::CloseAllTerminals);
            });
        } else if *action_id == BUILTIN_KILL_ALL_PROCESSES_ID {
            workspace.update(ctx, |_, ws_ctx| {
                ws_ctx.dispatch_typed_action(&WorkspaceAction::KillAllTerminalProcesses);
            });
        }
    }

    fn dispatch_action_to_group(
        action: &Action,
        group: &mut PaneGroup,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
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
