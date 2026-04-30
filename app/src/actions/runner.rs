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
/// The workspace pops these one at a time, waiting for
/// `terminal::Event::PendingCommandCompleted` before advancing to the next
/// step.  This ensures:
///
/// - Commands within an action run one after the other (each waits for the
///   previous shell block to complete before the next is submitted).
/// - A new terminal tab is opened for the next action only after all commands
///   in the current action have finished.
#[derive(Debug)]
pub enum TriggerQueueItem {
    /// Open a fresh terminal tab and make it the active target.
    OpenNewTab,
    /// Send this command to the currently-active terminal target.
    SendCommand(String),
}

pub struct TriggerRunner;

impl TriggerRunner {
    /// Build the sequential execution queue for a trigger and kick it off.
    ///
    /// Built-in actions (`CloseAllTerminals`, `KillAllTerminalProcesses`) are
    /// dispatched immediately as `WorkspaceAction`s (they have no shell
    /// commands to wait for).  All regular actions are decomposed into an
    /// `OpenNewTab` sentinel followed by one `SendCommand` entry per command.
    /// The workspace's `advance_trigger_queue` method drains the queue step
    /// by step, waiting for each command's block to complete before moving on.
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
                // Built-ins are instant — dispatch them right now so they run
                // before the first shell command in the queue.
                Self::dispatch_builtin(action_id, ctx);
                continue;
            }

            if action.commands.is_empty() {
                continue;
            }

            queue.push_back(TriggerQueueItem::OpenNewTab);
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
