use std::cell::RefCell;
use std::collections::HashMap;

use uuid::Uuid;
use warp_core::ui::Icon;
use warpui::{
    elements::{
        resizable_state_handle, ConstrainedBox, Container, CrossAxisAlignment, DragBarSide,
        Element, Flex, Hoverable, MainAxisAlignment, MainAxisSize, MouseStateHandle, ParentElement,
        Resizable, ResizableStateHandle, Shrinkable, Text,
    },
    fonts::{Properties, Weight},
    platform::Cursor,
    ui_components::components::UiComponent,
    AppContext, Entity, SingletonEntity, View, ViewContext,
};

use crate::appearance::Appearance;
use crate::drive::panel::{MAX_SIDEBAR_WIDTH_RATIO, MIN_SIDEBAR_WIDTH};
use crate::pane_group::pane::view::header::{components::HEADER_EDGE_PADDING, PANE_HEADER_HEIGHT};
use crate::ui_components::{
    buttons::{icon_button, icon_button_with_color},
    icons,
};
use crate::user_config::WarpConfig;
use crate::workspace::WorkspaceAction;

use super::model::{Action, SavedWorkspace, Trigger};

/// Per-row mouse state handles for one item in a dynamic list.
///
/// Stored in a `RefCell<HashMap<Uuid, RowMouseStates>>` so they survive
/// across re-renders without needing `&mut self` in `render()`.
struct RowMouseStates {
    primary: MouseStateHandle,
    secondary: MouseStateHandle,
    delete: MouseStateHandle,
}

impl Default for RowMouseStates {
    fn default() -> Self {
        Self {
            primary: Default::default(),
            secondary: Default::default(),
            delete: Default::default(),
        }
    }
}

/// The three tabs available inside the Actions & Triggers panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionsPanelTab {
    Actions,
    Triggers,
    Workspaces,
}

/// Top-level view for the Actions & Triggers side panel.
///
/// Renders a resizable panel with three switchable tabs: Actions, Triggers,
/// and Workspaces.  Reads live data from [`WarpConfig`] on every render so
/// the list always reflects the latest state on disk.
pub struct ActionsPanelView {
    resizable_state_handle: ResizableStateHandle,
    close_button_mouse_state: MouseStateHandle,
    actions_tab_mouse_state: MouseStateHandle,
    triggers_tab_mouse_state: MouseStateHandle,
    workspaces_tab_mouse_state: MouseStateHandle,
    save_workspace_mouse_state: MouseStateHandle,
    new_action_mouse_state: MouseStateHandle,
    new_trigger_mouse_state: MouseStateHandle,
    /// Stable per-row mouse states for action list rows (keyed by action UUID).
    action_row_states: RefCell<HashMap<Uuid, RowMouseStates>>,
    /// Stable per-row mouse states for trigger list rows (keyed by trigger UUID).
    trigger_row_states: RefCell<HashMap<Uuid, RowMouseStates>>,
    /// Stable per-row mouse states for workspace list rows (keyed by workspace UUID).
    workspace_row_states: RefCell<HashMap<Uuid, RowMouseStates>>,
    active_tab: ActionsPanelTab,
}

impl ActionsPanelView {
    pub fn new(_ctx: &mut ViewContext<Self>) -> Self {
        Self {
            resizable_state_handle: resizable_state_handle(340.0),
            close_button_mouse_state: Default::default(),
            actions_tab_mouse_state: Default::default(),
            triggers_tab_mouse_state: Default::default(),
            workspaces_tab_mouse_state: Default::default(),
            save_workspace_mouse_state: Default::default(),
            new_action_mouse_state: Default::default(),
            new_trigger_mouse_state: Default::default(),
            action_row_states: Default::default(),
            trigger_row_states: Default::default(),
            workspace_row_states: Default::default(),
            active_tab: ActionsPanelTab::Actions,
        }
    }

    pub fn set_active_tab(&mut self, tab: ActionsPanelTab, ctx: &mut ViewContext<Self>) {
        self.active_tab = tab;
        ctx.notify();
    }

    // ── Per-row mouse state helpers ────────────────────────────────────────

    fn action_states(&self, id: Uuid) -> (MouseStateHandle, MouseStateHandle, MouseStateHandle) {
        let mut map = self.action_row_states.borrow_mut();
        let s = map.entry(id).or_insert_with(RowMouseStates::default);
        (s.primary.clone(), s.secondary.clone(), s.delete.clone())
    }

    fn trigger_states(&self, id: Uuid) -> (MouseStateHandle, MouseStateHandle, MouseStateHandle) {
        let mut map = self.trigger_row_states.borrow_mut();
        let s = map.entry(id).or_insert_with(RowMouseStates::default);
        (s.primary.clone(), s.secondary.clone(), s.delete.clone())
    }

    fn workspace_states(&self, id: Uuid) -> (MouseStateHandle, MouseStateHandle) {
        let mut map = self.workspace_row_states.borrow_mut();
        let s = map.entry(id).or_insert_with(RowMouseStates::default);
        (s.primary.clone(), s.delete.clone())
    }

    // ── Rendering helpers ─────────────────────────────────────────────────

    fn render_header(&self, appearance: &Appearance) -> Box<dyn Element> {
        let close_button = {
            let icon_color = appearance
                .theme()
                .sub_text_color(appearance.theme().background());
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Close panel".to_string())
                .build()
                .finish();
            icon_button_with_color(
                appearance,
                icons::Icon::X,
                false,
                self.close_button_mouse_state.clone(),
                icon_color,
            )
            .with_tooltip(move || tooltip)
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(ActionsPanelAction::ClosePanel);
            })
            .finish()
        };

        let tab_row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(4.0)
            .with_child(self.render_tab_button(
                appearance,
                "Actions",
                ActionsPanelTab::Actions,
            ))
            .with_child(self.render_tab_button(
                appearance,
                "Triggers",
                ActionsPanelTab::Triggers,
            ))
            .with_child(self.render_tab_button(
                appearance,
                "Workspaces",
                ActionsPanelTab::Workspaces,
            ))
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        Container::new(
            ConstrainedBox::new(
                Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(Shrinkable::new(1.0, tab_row).finish())
                    .with_child(close_button)
                    .finish(),
            )
            .with_height(PANE_HEADER_HEIGHT)
            .finish(),
        )
        .with_padding_left(10.)
        .with_padding_right(HEADER_EDGE_PADDING)
        .finish()
    }

    fn render_tab_button(
        &self,
        appearance: &Appearance,
        label: &'static str,
        tab: ActionsPanelTab,
    ) -> Box<dyn Element> {
        let is_active = self.active_tab == tab;
        let theme = appearance.theme();
        let text_color = if is_active {
            theme.main_text_color(theme.background())
        } else {
            theme.sub_text_color(theme.background())
        };

        let weight = if is_active {
            Weight::Semibold
        } else {
            Weight::Normal
        };
        let text = Text::new(label, appearance.ui_font_family(), 12.)
            .with_style(Properties::default().weight(weight))
            .with_color(text_color.into_solid())
            .finish();

        let mouse_state = match tab {
            ActionsPanelTab::Actions => self.actions_tab_mouse_state.clone(),
            ActionsPanelTab::Triggers => self.triggers_tab_mouse_state.clone(),
            ActionsPanelTab::Workspaces => self.workspaces_tab_mouse_state.clone(),
        };

        Hoverable::new(mouse_state, move |_| {
            Container::new(text)
                .with_padding_left(6.)
                .with_padding_right(6.)
                .with_padding_top(4.)
                .with_padding_bottom(4.)
                .finish()
        })
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(ActionsPanelAction::SetTab(tab));
        })
        .with_cursor(Cursor::PointingHand)
        .finish()
    }

    fn render_actions_tab(&self, actions: &[Action], appearance: &Appearance) -> Box<dyn Element> {
        let new_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Create new action".to_string())
                .build()
                .finish();
            icon_button(
                appearance,
                Icon::Plus,
                false,
                self.new_action_mouse_state.clone(),
            )
            .with_tooltip(move || tooltip)
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(ActionsPanelAction::NewAction);
            })
            .finish()
        };

        let toolbar = Container::new(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_main_axis_alignment(MainAxisAlignment::End)
                .with_child(new_button)
                .finish(),
        )
        .with_padding_left(10.)
        .with_padding_right(10.)
        .with_padding_bottom(4.)
        .finish();

        let list: Box<dyn Element> = if actions.is_empty() {
            self.render_empty_state(appearance, "No actions yet.")
        } else {
            let mut col = Flex::column().with_main_axis_size(MainAxisSize::Max);
            for action in actions {
                col = col.with_child(self.render_action_row(action, appearance));
            }
            Shrinkable::new(1.0, col.finish()).finish()
        };

        Flex::column()
            .with_child(toolbar)
            .with_child(Shrinkable::new(1.0, list).finish())
            .with_main_axis_size(MainAxisSize::Max)
            .finish()
    }

    fn render_action_row(&self, action: &Action, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let font = appearance.ui_font_family();

        let name = Text::new(action.name.clone(), font, 13.)
            .with_style(Properties::default().weight(Weight::Medium))
            .with_color(theme.main_text_color(theme.background()).into_solid())
            .finish();

        let cmd_count = format!(
            "{} command{}",
            action.commands.len(),
            if action.commands.len() == 1 { "" } else { "s" }
        );
        let meta = Text::new(cmd_count, font, 11.)
            .with_color(theme.sub_text_color(theme.background()).into_solid())
            .finish();

        let action_id = action.id;
        let (run_state, edit_state, delete_state) = self.action_states(action_id);

        let run_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Run in active terminal".to_string())
                .build()
                .finish();
            icon_button(appearance, Icon::Play, false, run_state)
                .with_tooltip(move || tooltip)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::RunAction(action_id));
                })
                .finish()
        };

        let edit_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Edit action file".to_string())
                .build()
                .finish();
            let icon_color = theme.sub_text_color(theme.background());
            icon_button_with_color(appearance, Icon::Edit, false, edit_state, icon_color)
                .with_tooltip(move || tooltip)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::EditAction(action_id));
                })
                .finish()
        };

        let delete_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Delete action".to_string())
                .build()
                .finish();
            let icon_color = theme.sub_text_color(theme.background());
            icon_button_with_color(appearance, Icon::Trash, false, delete_state, icon_color)
                .with_tooltip(move || tooltip)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::DeleteAction(action_id));
                })
                .finish()
        };

        let buttons = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(2.0)
            .with_child(run_button)
            .with_child(edit_button)
            .with_child(delete_button)
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        Container::new(
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                .with_main_axis_size(MainAxisSize::Max)
                .with_child(
                    Flex::column()
                        .with_child(name)
                        .with_child(meta)
                        .with_main_axis_size(MainAxisSize::Min)
                        .finish(),
                )
                .with_child(buttons)
                .finish(),
        )
        .with_padding_left(10.)
        .with_padding_right(10.)
        .with_padding_top(6.)
        .with_padding_bottom(6.)
        .finish()
    }

    fn render_triggers_tab(
        &self,
        triggers: &[Trigger],
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let new_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Create new trigger".to_string())
                .build()
                .finish();
            icon_button(
                appearance,
                Icon::Plus,
                false,
                self.new_trigger_mouse_state.clone(),
            )
            .with_tooltip(move || tooltip)
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(ActionsPanelAction::NewTrigger);
            })
            .finish()
        };

        let toolbar = Container::new(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_main_axis_alignment(MainAxisAlignment::End)
                .with_child(new_button)
                .finish(),
        )
        .with_padding_left(10.)
        .with_padding_right(10.)
        .with_padding_bottom(4.)
        .finish();

        let list: Box<dyn Element> = if triggers.is_empty() {
            self.render_empty_state(appearance, "No triggers yet.")
        } else {
            let mut col = Flex::column().with_main_axis_size(MainAxisSize::Max);
            for trigger in triggers {
                col = col.with_child(self.render_trigger_row(trigger, appearance));
            }
            Shrinkable::new(1.0, col.finish()).finish()
        };

        Flex::column()
            .with_child(toolbar)
            .with_child(Shrinkable::new(1.0, list).finish())
            .with_main_axis_size(MainAxisSize::Max)
            .finish()
    }

    fn render_trigger_row(&self, trigger: &Trigger, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let font = appearance.ui_font_family();

        let name = Text::new(trigger.name.clone(), font, 13.)
            .with_style(Properties::default().weight(Weight::Medium))
            .with_color(theme.main_text_color(theme.background()).into_solid())
            .finish();

        let action_count = format!(
            "{} action{}",
            trigger.action_ids.len(),
            if trigger.action_ids.len() == 1 { "" } else { "s" }
        );
        let meta = Text::new(action_count, font, 11.)
            .with_color(theme.sub_text_color(theme.background()).into_solid())
            .finish();

        let trigger_id = trigger.id;
        let (run_state, edit_state, delete_state) = self.trigger_states(trigger_id);

        let run_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Run trigger across targets".to_string())
                .build()
                .finish();
            icon_button(appearance, Icon::Play, false, run_state)
                .with_tooltip(move || tooltip)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::RunTrigger(trigger_id));
                })
                .finish()
        };

        let edit_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Edit trigger file".to_string())
                .build()
                .finish();
            let icon_color = theme.sub_text_color(theme.background());
            icon_button_with_color(appearance, Icon::Edit, false, edit_state, icon_color)
                .with_tooltip(move || tooltip)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::EditTrigger(trigger_id));
                })
                .finish()
        };

        let delete_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Delete trigger".to_string())
                .build()
                .finish();
            let icon_color = theme.sub_text_color(theme.background());
            icon_button_with_color(appearance, Icon::Trash, false, delete_state, icon_color)
                .with_tooltip(move || tooltip)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::DeleteTrigger(trigger_id));
                })
                .finish()
        };

        let buttons = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(2.0)
            .with_child(run_button)
            .with_child(edit_button)
            .with_child(delete_button)
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        Container::new(
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                .with_main_axis_size(MainAxisSize::Max)
                .with_child(
                    Flex::column()
                        .with_child(name)
                        .with_child(meta)
                        .with_main_axis_size(MainAxisSize::Min)
                        .finish(),
                )
                .with_child(buttons)
                .finish(),
        )
        .with_padding_left(10.)
        .with_padding_right(10.)
        .with_padding_top(6.)
        .with_padding_bottom(6.)
        .finish()
    }

    fn render_workspaces_tab(
        &self,
        workspaces: &[SavedWorkspace],
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let save_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Save current window layout as workspace".to_string())
                .build()
                .finish();
            icon_button(
                appearance,
                Icon::Plus,
                false,
                self.save_workspace_mouse_state.clone(),
            )
            .with_tooltip(move || tooltip)
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(ActionsPanelAction::SaveWorkspace);
            })
            .finish()
        };

        let save_row = Container::new(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_main_axis_alignment(MainAxisAlignment::End)
                .with_child(save_button)
                .finish(),
        )
        .with_padding_left(10.)
        .with_padding_right(10.)
        .with_padding_bottom(4.)
        .finish();

        let list: Box<dyn Element> = if workspaces.is_empty() {
            self.render_empty_state(appearance, "No saved workspaces.")
        } else {
            let mut col = Flex::column().with_main_axis_size(MainAxisSize::Max);
            for ws in workspaces {
                col = col.with_child(self.render_workspace_row(ws, appearance));
            }
            Shrinkable::new(1.0, col.finish()).finish()
        };

        Flex::column()
            .with_child(save_row)
            .with_child(Shrinkable::new(1.0, list).finish())
            .with_main_axis_size(MainAxisSize::Max)
            .finish()
    }

    fn render_workspace_row(
        &self,
        workspace: &SavedWorkspace,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let font = appearance.ui_font_family();

        let name = Text::new(workspace.name.clone(), font, 13.)
            .with_style(Properties::default().weight(Weight::Medium))
            .with_color(theme.main_text_color(theme.background()).into_solid())
            .finish();

        let tab_count = workspace.snapshot.tabs.len();
        let meta = Text::new(
            format!("{} tab{}", tab_count, if tab_count == 1 { "" } else { "s" }),
            font,
            11.,
        )
        .with_color(theme.sub_text_color(theme.background()).into_solid())
        .finish();

        let workspace_id = workspace.id;
        let (restore_state, delete_state) = self.workspace_states(workspace_id);

        let restore_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Restore workspace".to_string())
                .build()
                .finish();
            icon_button(appearance, Icon::Refresh, false, restore_state)
                .with_tooltip(move || tooltip)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::RestoreWorkspace(workspace_id));
                })
                .finish()
        };

        let delete_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder
                .tool_tip("Delete workspace".to_string())
                .build()
                .finish();
            let icon_color = theme.sub_text_color(theme.background());
            icon_button_with_color(appearance, Icon::Trash, false, delete_state, icon_color)
                .with_tooltip(move || tooltip)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::DeleteWorkspace(workspace_id));
                })
                .finish()
        };

        let buttons = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(2.0)
            .with_child(restore_button)
            .with_child(delete_button)
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        Container::new(
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                .with_main_axis_size(MainAxisSize::Max)
                .with_child(
                    Flex::column()
                        .with_child(name)
                        .with_child(meta)
                        .with_main_axis_size(MainAxisSize::Min)
                        .finish(),
                )
                .with_child(buttons)
                .finish(),
        )
        .with_padding_left(10.)
        .with_padding_right(10.)
        .with_padding_top(6.)
        .with_padding_bottom(6.)
        .finish()
    }

    fn render_empty_state(&self, appearance: &Appearance, message: &'static str) -> Box<dyn Element> {
        let sub_text = appearance
            .theme()
            .sub_text_color(appearance.theme().background())
            .into_solid();

        Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_main_axis_size(MainAxisSize::Max)
            .with_child(
                Text::new(message, appearance.ui_font_family(), 13.)
                    .with_color(sub_text)
                    .finish(),
            )
            .finish()
    }
}

// ── Action type dispatched by button clicks inside the panel ──────────────────

#[derive(Clone, Debug)]
pub enum ActionsPanelAction {
    SetTab(ActionsPanelTab),
    ClosePanel,
    RunAction(Uuid),
    RunTrigger(Uuid),
    SaveWorkspace,
    RestoreWorkspace(Uuid),
    DeleteWorkspace(Uuid),
    NewAction,
    NewTrigger,
    DeleteAction(Uuid),
    DeleteTrigger(Uuid),
    EditAction(Uuid),
    EditTrigger(Uuid),
}

// ── View impl ─────────────────────────────────────────────────────────────────

impl Entity for ActionsPanelView {
    type Event = ();
}

impl View for ActionsPanelView {
    fn ui_name() -> &'static str {
        "ActionsPanelView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let warp_config = WarpConfig::as_ref(app);

        let actions = warp_config.actions().to_vec();
        let triggers = warp_config.triggers().to_vec();
        let workspaces = warp_config.saved_workspaces().to_vec();
        drop(warp_config);

        let header = self.render_header(appearance);

        let content: Box<dyn Element> = match self.active_tab {
            ActionsPanelTab::Actions => self.render_actions_tab(&actions, appearance),
            ActionsPanelTab::Triggers => self.render_triggers_tab(&triggers, appearance),
            ActionsPanelTab::Workspaces => self.render_workspaces_tab(&workspaces, appearance),
        };

        let panel_content = Container::new(
            Flex::column()
                .with_child(header)
                .with_child(Shrinkable::new(1.0, content).finish())
                .with_main_axis_size(MainAxisSize::Max)
                .finish(),
        )
        .finish();

        if warpui::platform::is_mobile_device() {
            return panel_content;
        }

        Resizable::new(self.resizable_state_handle.clone(), panel_content)
            .with_dragbar_side(DragBarSide::Left)
            .on_resize(move |ctx, _| {
                ctx.notify();
            })
            .with_bounds_callback(Box::new(|window_size| {
                let min_width = MIN_SIDEBAR_WIDTH;
                let max_width = window_size.x() * MAX_SIDEBAR_WIDTH_RATIO;
                (min_width, max_width.max(min_width))
            }))
            .finish()
    }
}

impl warpui::TypedActionView for ActionsPanelView {
    type Action = ActionsPanelAction;

    fn handle_action(&mut self, action: &ActionsPanelAction, ctx: &mut ViewContext<Self>) {
        match action {
            ActionsPanelAction::SetTab(tab) => {
                self.set_active_tab(*tab, ctx);
            }
            ActionsPanelAction::ClosePanel => {
                ctx.dispatch_typed_action(&WorkspaceAction::ToggleActionsPanel);
            }
            ActionsPanelAction::RunAction(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::RunActionInActiveTerminal(*id));
            }
            ActionsPanelAction::RunTrigger(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::RunTrigger(*id));
            }
            ActionsPanelAction::SaveWorkspace => {
                ctx.dispatch_typed_action(&WorkspaceAction::SaveCurrentWorkspace);
            }
            ActionsPanelAction::RestoreWorkspace(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::RestoreWorkspace(*id));
            }
            ActionsPanelAction::DeleteWorkspace(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::DeleteWorkspace(*id));
            }
            ActionsPanelAction::NewAction => {
                ctx.dispatch_typed_action(&WorkspaceAction::NewAction);
            }
            ActionsPanelAction::NewTrigger => {
                ctx.dispatch_typed_action(&WorkspaceAction::NewTrigger);
            }
            ActionsPanelAction::DeleteAction(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::DeleteAction(*id));
            }
            ActionsPanelAction::DeleteTrigger(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::DeleteTrigger(*id));
            }
            ActionsPanelAction::EditAction(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::OpenActionFile(*id));
            }
            ActionsPanelAction::EditTrigger(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::OpenTriggerFile(*id));
            }
        }
    }
}
