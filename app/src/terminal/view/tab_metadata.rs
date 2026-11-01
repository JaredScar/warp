use crate::context_chips::display_chip::GitLineChanges;
use crate::context_chips::{git_line_changes_from_chips, ContextChipKind};
use crate::terminal::model::terminal_model::TerminalModel;
use crate::terminal::TerminalView;
use warpui::AppContext;

/// Snapshot of one terminal for the sessions board.
pub struct TerminalLiveStatus {
    pub running: bool,
    pub command: Option<String>,
    pub exit_code: Option<i32>,
    pub cwd: Option<String>,
    pub branch: Option<String>,
}

fn latest_finished_user_command_from(model: &TerminalModel) -> Option<(String, i32)> {
    model.block_list().blocks().iter().rev().find_map(|block| {
        if block.is_done()
            && !block.is_background()
            && !block.is_static()
            && !block.is_in_band_command_block()
            && (block.bootstrap_stage().is_done() || block.is_restored())
        {
            let cmd = block.command_to_string();
            if cmd.trim().is_empty() {
                None
            } else {
                Some((cmd, block.exit_code().value()))
            }
        } else {
            None
        }
    })
}

impl TerminalView {
    fn prompt_chip_value(&self, chip_kind: &ContextChipKind, ctx: &AppContext) -> Option<String> {
        self.current_prompt
            .as_ref(ctx)
            .latest_chip_value(chip_kind, ctx)
            .map(|v| v.to_string())
            .filter(|value| !value.trim().is_empty())
    }

    pub fn display_working_directory(&self, ctx: &AppContext) -> Option<String> {
        let raw = self
            .prompt_chip_value(&ContextChipKind::WorkingDirectory, ctx)
            .or_else(|| self.pwd())?;
        let home_dir = self
            .active_block_session_id()
            .and_then(|session_id| self.sessions.as_ref(ctx).get(session_id))
            .and_then(|session| session.home_dir().map(str::to_owned));
        Some(warp_util::path::user_friendly_path(&raw, home_dir.as_deref()).to_string())
    }

    pub fn terminal_title_from_shell(&self) -> String {
        let model = self.model.lock();
        let fallback_title = model.shell_launch_state().display_name().to_owned();
        model
            .terminal_title()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(fallback_title)
    }

    #[cfg_attr(not(feature = "local_fs"), allow(clippy::unnecessary_lazy_evaluations))]
    pub fn current_git_branch(&self, ctx: &AppContext) -> Option<String> {
        self.prompt_chip_value(&ContextChipKind::ShellGitBranch, ctx)
            .or_else(|| {
                #[cfg(feature = "local_fs")]
                {
                    self.git_status_metadata(ctx)
                        .map(|metadata| metadata.current_branch_name.clone())
                        .filter(|branch| !branch.trim().is_empty())
                }
                #[cfg(not(feature = "local_fs"))]
                {
                    None
                }
            })
    }

    /// Exit code of the most recent finished user command, if one exists.
    pub fn latest_user_exit_code(&self) -> Option<i32> {
        self.latest_finished_user_command().map(|(_, exit_code)| exit_code)
    }

    /// Whether a command is executing, the command text to show, and the last
    /// exit code when the shell is idle.
    pub fn live_status(&self, ctx: &AppContext) -> TerminalLiveStatus {
        let cwd = self.display_working_directory(ctx);
        let branch = self.current_git_branch(ctx);
        let model = self.model.lock();
        let (running, running_command) = {
            let active = model.block_list().active_block();
            let running = active.is_executing();
            let running_command = if running {
                let cmd = active.command_to_string();
                let cmd = cmd.trim();
                if cmd.is_empty() {
                    None
                } else {
                    Some(cmd.to_string())
                }
            } else {
                None
            };
            (running, running_command)
        };
        let finished = latest_finished_user_command_from(&model);
        let command = running_command.or_else(|| finished.as_ref().map(|(cmd, _)| cmd.clone()));
        let exit_code = if running {
            None
        } else {
            finished.map(|(_, exit_code)| exit_code)
        };
        TerminalLiveStatus {
            running,
            command,
            exit_code,
            cwd,
            branch,
        }
    }

    fn latest_finished_user_command(&self) -> Option<(String, i32)> {
        let model = self.model.lock();
        latest_finished_user_command_from(&model)
    }

    pub fn last_completed_command_text(&self) -> Option<String> {
        let model = self.model.lock();
        model.block_list().blocks().iter().rev().find_map(|block| {
            if block.finished()
                && !block.is_background()
                && !block.is_static()
                && (block.bootstrap_stage().is_done() || block.is_restored())
            {
                let cmd = block.command_to_string();
                if cmd.trim().is_empty() {
                    None
                } else {
                    Some(cmd)
                }
            } else {
                None
            }
        })
    }

    pub fn terminal_title_text(&self) -> String {
        if !self.terminal_title.trim().is_empty() {
            return self.terminal_title.clone();
        }
        self.terminal_title_from_shell()
    }

    pub fn current_pull_request_url(&self, ctx: &AppContext) -> Option<String> {
        self.current_prompt
            .as_ref(ctx)
            .latest_chip_value(&ContextChipKind::GithubPullRequest, ctx)
            .map(|v| v.to_string())
            .filter(|value| !value.trim().is_empty())
    }

    #[cfg_attr(not(feature = "local_fs"), allow(clippy::unnecessary_lazy_evaluations))]
    pub fn current_diff_line_changes(&self, ctx: &AppContext) -> Option<GitLineChanges> {
        // Prefer the filesystem-event-based GitRepoStatusModel (which includes
        // untracked files) over parsing the raw shell chip output. This matches
        // the preference order used by the prompt chip display (display.rs) and
        // agent footer (chips.rs).
        #[cfg(feature = "local_fs")]
        let from_model = self
            .git_status_metadata(ctx)
            .map(|metadata| GitLineChanges::from_diff_stats(&metadata.stats_against_head));
        #[cfg(not(feature = "local_fs"))]
        let from_model: Option<GitLineChanges> = None;

        from_model
            .or_else(|| {
                git_line_changes_from_chips(&self.current_prompt.as_ref(ctx).agent_view_chips(ctx))
            })
            .filter(|line_changes| {
                line_changes.files_changed > 0
                    || line_changes.lines_added > 0
                    || line_changes.lines_removed > 0
            })
    }
}
