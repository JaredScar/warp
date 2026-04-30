use std::collections::VecDeque;

use warpui::ViewContext;

use crate::workspace::Workspace;
use crate::WorkspaceAction;

use super::model::{
    is_builtin_action, Action, Trigger,
    BUILTIN_CLOSE_ALL_TERMINALS_ID, BUILTIN_KILL_ALL_PROCESSES_ID,
};

/// A single step in the sequential trigger execution queue.
///
/// The workspace pops one item at a time, waiting for either
/// `terminal::Event::PendingCommandCompleted` or a 5-second fallback timer
/// before advancing to the next step.  This ensures:
///
/// - Commands within an action run one after the other.
/// - The next action's tab is opened only after the last command of the
///   current action completes (or times out — for long-running processes).
#[derive(Debug)]
pub enum TriggerQueueItem {
    /// Open a fresh terminal tab, then optionally rename it.
    OpenNewTab {
        /// If `Some`, rename the newly opened tab to this string.
        tab_name: Option<String>,
    },
    /// Send this command to the currently-active terminal target.
    SendCommand(String),
}

pub struct TriggerRunner;

impl TriggerRunner {
    /// Build the sequential execution queue for a trigger and kick it off.
    ///
    /// Built-in actions are dispatched immediately as `WorkspaceAction`s.
    /// All regular actions are decomposed into:
    ///   `OpenNewTab { tab_name } → SendCommand(cmd1) → SendCommand(cmd2) → …`
    pub fn build_queue(
        trigger: &Trigger,
        all_actions: &[Action],
        workspace: &mut Workspace,
        ctx: &mut ViewContext<Workspace>,
    ) {
        let mut queue: VecDeque<TriggerQueueItem> = VecDeque::new();

        for action_id in &trigger.action_ids {
            let Some(action) = all_actions.iter().find(|a| &a.id == action_id) else {
                continue;
            };

            if is_builtin_action(action_id) {
                Self::dispatch_builtin(action_id, ctx);
                continue;
            }

            if action.commands.is_empty() {
                continue;
            }

            queue.push_back(TriggerQueueItem::OpenNewTab {
                tab_name: action.tab_name.clone(),
            });
            for cmd in &action.commands {
                if !cmd.trim().is_empty() {
                    queue.push_back(TriggerQueueItem::SendCommand(cmd.clone()));
                }
            }
        }

        workspace.trigger_queue = queue;
        workspace.advance_trigger_queue(ctx);
    }

    fn dispatch_builtin(action_id: &uuid::Uuid, ctx: &mut ViewContext<Workspace>) {
        if *action_id == BUILTIN_CLOSE_ALL_TERMINALS_ID {
            ctx.dispatch_typed_action(&WorkspaceAction::CloseAllTerminals);
        } else if *action_id == BUILTIN_KILL_ALL_PROCESSES_ID {
            ctx.dispatch_typed_action(&WorkspaceAction::KillAllTerminalProcesses);
        }
    }
}
