use std::collections::VecDeque;

use warpui::ViewContext;

use crate::workspace::Workspace;
use crate::WorkspaceAction;

use super::model::{
    is_builtin_action, Action, Trigger, BUILTIN_CLOSE_ALL_TERMINALS_ID,
    BUILTIN_KILL_ALL_PROCESSES_ID,
};

/// A single step in the sequential trigger execution queue.
///
/// The workspace pops one item at a time, waiting for either
/// `terminal::Event::PendingCommandCompleted` or the fallback timer before
/// advancing to the next step.  This ensures:
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
    SendCommand {
        /// The shell command to execute.
        cmd: String,
        /// Max seconds to wait before advancing.  `None` means use the
        /// global default (5 s). Ignored when `wait_until_exit` is set.
        timeout_secs: Option<u64>,
        /// Runbook step this command belongs to, if any.
        runbook_step_id: Option<uuid::Uuid>,
        /// Wait for the command to exit instead of moving on at the timeout.
        /// Runbook steps set this so a slow command is not marked finished early.
        wait_until_exit: bool,
    },
}

/// How far a Run All should proceed, based on the steps' current outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListedStep {
    NotRun,
    Running,
    Settled,
}

/// What Run All should do with the current step outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunAllPlan {
    /// A step is still running, or there is nothing to run.
    Wait,
    /// Every step has already finished. Clear them and start over.
    Restart,
    /// Run from this index through the end. Earlier steps keep their results.
    From(usize),
}

pub fn plan_run_all(steps: &[ListedStep]) -> RunAllPlan {
    if steps.is_empty() || steps.iter().any(|step| *step == ListedStep::Running) {
        return RunAllPlan::Wait;
    }
    if let Some(index) = steps
        .iter()
        .position(|step| *step == ListedStep::NotRun)
    {
        return RunAllPlan::From(index);
    }
    RunAllPlan::Restart
}

/// A missing exit code halts the run. Only `0` counts as passed.
pub fn runbook_step_failed(exit_code: Option<i32>) -> bool {
    exit_code != Some(0)
}

/// Split a command field into individually submitted lines.
///
/// Each line is executed with its own Enter. A newline inside one PTY write
/// is pasted as text when bracketed paste is enabled, so `dotnet restore`
/// and `dotnet run` would otherwise stay on the same line.
pub fn split_command_lines(command: &str) -> Vec<String> {
    command
        .split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect()
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
                for line in split_command_lines(cmd) {
                    queue.push_back(TriggerQueueItem::SendCommand {
                        cmd: line,
                        timeout_secs: action.timeout_secs,
                        runbook_step_id: None,
                        wait_until_exit: false,
                    });
                }
            }
        }

        workspace.trigger_queue = queue;
        workspace.trigger_running_name = Some(trigger.name.clone());
        workspace.trigger_show_overlay = true;
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

#[cfg(test)]
mod tests {
    use super::split_command_lines;

    #[test]
    fn each_line_is_its_own_command() {
        assert_eq!(
            split_command_lines("dotnet restore\ndotnet run\r\n"),
            vec!["dotnet restore".to_string(), "dotnet run".to_string()]
        );
    }

    #[test]
    fn a_single_command_stays_intact() {
        assert_eq!(
            split_command_lines("dotnet run"),
            vec!["dotnet run".to_string()]
        );
    }

    #[test]
    fn run_all_starts_at_the_first_step_that_has_not_run() {
        use super::{plan_run_all, ListedStep, RunAllPlan};
        assert_eq!(
            plan_run_all(&[
                ListedStep::Settled,
                ListedStep::NotRun,
                ListedStep::NotRun,
            ]),
            RunAllPlan::From(1)
        );
    }

    #[test]
    fn run_all_restarts_when_every_step_has_finished() {
        use super::{plan_run_all, ListedStep, RunAllPlan};
        assert_eq!(
            plan_run_all(&[ListedStep::Settled, ListedStep::Settled]),
            RunAllPlan::Restart
        );
    }

    #[test]
    fn run_all_waits_while_a_step_is_running() {
        use super::{plan_run_all, ListedStep, RunAllPlan};
        assert_eq!(
            plan_run_all(&[ListedStep::Settled, ListedStep::Running]),
            RunAllPlan::Wait
        );
    }

    #[test]
    fn only_a_zero_exit_code_passes() {
        use super::runbook_step_failed;
        assert!(!runbook_step_failed(Some(0)));
        assert!(runbook_step_failed(Some(1)));
        assert!(runbook_step_failed(None));
    }
}
