use std::cell::RefCell;
use std::collections::HashMap;

use uuid::Uuid;
use crate::actions::model::is_builtin_action;
use warp_core::ui::Icon;
use warpui::{
    elements::{
        resizable_state_handle, Align, ConstrainedBox, Container, CornerRadius,
        CrossAxisAlignment, DragBarSide, Element, Flex, Hoverable, MainAxisAlignment, MainAxisSize,
        MouseStateHandle, ParentElement, Radius, Resizable, ResizableStateHandle, Shrinkable, Text,
    },
    fonts::{Properties, Weight},
    platform::Cursor,
    ui_components::{
        button::ButtonVariant,
        components::{Coords, UiComponent, UiComponentStyles},
    },
    AppContext, SingletonEntity, View, ViewContext, ViewHandle,
};

use crate::appearance::Appearance;
use crate::drive::panel::{MAX_SIDEBAR_WIDTH_RATIO, MIN_SIDEBAR_WIDTH};
use crate::editor::{EditorView, Event as EditorEvent, SingleLineEditorOptions, TextOptions};
use crate::pane_group::pane::view::header::{components::HEADER_EDGE_PADDING, PANE_HEADER_HEIGHT};
use crate::ui_components::{
    blended_colors,
    buttons::{icon_button, icon_button_with_color},
    icons,
};
use crate::user_config::WarpConfig;
use crate::workspace::WorkspaceAction;

use super::model::{Action, SavedWorkspace, Trigger};
use super::storage;

// ── Constants ─────────────────────────────────────────────────────────────────

const FORM_PADDING: f32 = 14.;
const FIELD_SPACING: f32 = 12.;
const LABEL_SIZE: f32 = 11.;
const FIELD_HEIGHT: f32 = 32.;
const BUTTON_FONT_SIZE: f32 = 13.;

// ── Panel mode ────────────────────────────────────────────────────────────────

/// Tracks whether the panel shows the list or an inline editor form.
#[derive(Clone, Debug)]
enum PanelMode {
    List,
    /// Editing an action.  `None` means a brand-new action; `Some(id)` means editing existing.
    EditAction(Option<Uuid>),
    /// Editing a trigger.  `None` means a brand-new trigger; `Some(id)` means editing existing.
    EditTrigger(Option<Uuid>),
    /// Naming/renaming a workspace.  `None` = new workspace; `Some(id)` = rename existing.
    EditWorkspaceName(Option<Uuid>),
}

// ── Per-row stable mouse state ─────────────────────────────────────────────

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

// ── View struct ───────────────────────────────────────────────────────────────

pub struct ActionsPanelView {
    // ── layout ──
    resizable_state_handle: ResizableStateHandle,
    // ── header ──
    actions_tab_mouse_state: MouseStateHandle,
    triggers_tab_mouse_state: MouseStateHandle,
    workspaces_tab_mouse_state: MouseStateHandle,
    // ── list tab buttons ──
    save_workspace_mouse_state: MouseStateHandle,
    new_action_mouse_state: MouseStateHandle,
    new_trigger_mouse_state: MouseStateHandle,
    active_tab: ActionsPanelTab,
    // ── stable per-row mouse states (keyed by UUID) ──
    action_row_states: RefCell<HashMap<Uuid, RowMouseStates>>,
    trigger_row_states: RefCell<HashMap<Uuid, RowMouseStates>>,
    workspace_row_states: RefCell<HashMap<Uuid, RowMouseStates>>,
    // ── editor form ──
    panel_mode: PanelMode,
    edit_name_editor: ViewHandle<EditorView>,
    edit_desc_editor: ViewHandle<EditorView>,
    /// One single-line editor per command, each paired with a stable UUID for mouse-state keying.
    edit_command_editors: Vec<(Uuid, ViewHandle<EditorView>)>,
    /// Per-command delete-button mouse states, keyed by the command's stable UUID.
    edit_command_remove_states: RefCell<HashMap<Uuid, MouseStateHandle>>,
    /// Mouse state for the "+ Add Command" button.
    add_command_state: MouseStateHandle,
    /// Single-line editor used for the workspace name form.
    edit_workspace_name_editor: ViewHandle<EditorView>,
    /// Ordered list of action IDs selected for the trigger being edited.
    edit_selected_action_ids: Vec<Uuid>,
    /// Search query typed in the trigger action picker.
    trigger_search_query: String,
    /// Single-line editor for filtering available actions in the trigger form.
    trigger_search_editor: ViewHandle<EditorView>,
    /// Stable mouse states for the picker rows (add button, keyed by UUID).
    edit_action_toggle_states: RefCell<HashMap<Uuid, MouseStateHandle>>,
    /// Stable mouse states for selected-list control buttons (up/down/remove, keyed by UUID).
    edit_selected_move_up_states: RefCell<HashMap<Uuid, MouseStateHandle>>,
    edit_selected_move_down_states: RefCell<HashMap<Uuid, MouseStateHandle>>,
    edit_selected_remove_states: RefCell<HashMap<Uuid, MouseStateHandle>>,
    save_form_state: MouseStateHandle,
    cancel_form_state: MouseStateHandle,
}

impl ActionsPanelView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let appearance = Appearance::as_ref(ctx);
        let font_size = appearance.ui_font_size();
        drop(appearance);

        let single_line_opts = SingleLineEditorOptions {
            text: TextOptions {
                font_size_override: Some(font_size),
                ..Default::default()
            },
            ..Default::default()
        };

        let edit_name_editor =
            ctx.add_typed_action_view(|ctx| EditorView::single_line(single_line_opts.clone(), ctx));
        let edit_desc_editor =
            ctx.add_typed_action_view(|ctx| EditorView::single_line(single_line_opts.clone(), ctx));
        let trigger_search_editor =
            ctx.add_typed_action_view(|ctx| EditorView::single_line(single_line_opts.clone(), ctx));

        edit_name_editor.update(ctx, |editor, ctx| {
            editor.set_placeholder_text("Name (required)", ctx);
        });
        edit_desc_editor.update(ctx, |editor, ctx| {
            editor.set_placeholder_text("Description (optional)", ctx);
        });
        trigger_search_editor.update(ctx, |editor, ctx| {
            editor.set_placeholder_text("Search actions…", ctx);
        });

        // Keep trigger_search_query in sync with the search editor text.
        ctx.subscribe_to_view(&trigger_search_editor, |me, _, event, ctx| {
            if matches!(event, EditorEvent::Edited(_)) {
                me.trigger_search_query = me
                    .trigger_search_editor
                    .read(ctx, |e, ctx| e.buffer_text(ctx));
                ctx.notify();
            }
        });

        let edit_workspace_name_editor =
            ctx.add_typed_action_view(|ctx| EditorView::single_line(single_line_opts.clone(), ctx));
        edit_workspace_name_editor.update(ctx, |editor, ctx| {
            editor.set_placeholder_text("Workspace name (required)", ctx);
        });

        Self {
            resizable_state_handle: resizable_state_handle(360.0),
            actions_tab_mouse_state: Default::default(),
            triggers_tab_mouse_state: Default::default(),
            workspaces_tab_mouse_state: Default::default(),
            save_workspace_mouse_state: Default::default(),
            new_action_mouse_state: Default::default(),
            new_trigger_mouse_state: Default::default(),
            active_tab: ActionsPanelTab::Actions,
            action_row_states: Default::default(),
            trigger_row_states: Default::default(),
            workspace_row_states: Default::default(),
            panel_mode: PanelMode::List,
            edit_name_editor,
            edit_desc_editor,
            edit_command_editors: Vec::new(),
            edit_command_remove_states: Default::default(),
            add_command_state: Default::default(),
            edit_workspace_name_editor,
            edit_selected_action_ids: Vec::new(),
            trigger_search_query: String::new(),
            trigger_search_editor,
            edit_action_toggle_states: Default::default(),
            edit_selected_move_up_states: Default::default(),
            edit_selected_move_down_states: Default::default(),
            edit_selected_remove_states: Default::default(),
            save_form_state: Default::default(),
            cancel_form_state: Default::default(),
        }
    }

    pub fn set_active_tab(&mut self, tab: ActionsPanelTab, ctx: &mut ViewContext<Self>) {
        self.active_tab = tab;
        ctx.notify();
    }

    // ── Form open/populate ────────────────────────────────────────────────

    /// Create a fresh single-line command editor with placeholder text.
    fn make_command_editor(
        &self,
        ctx: &mut ViewContext<Self>,
    ) -> ViewHandle<EditorView> {
        let appearance = Appearance::as_ref(ctx);
        let font_size = appearance.ui_font_size();
        drop(appearance);
        let opts = SingleLineEditorOptions {
            text: TextOptions { font_size_override: Some(font_size), ..Default::default() },
            ..Default::default()
        };
        let editor = ctx.add_typed_action_view(|ctx| EditorView::single_line(opts, ctx));
        editor.update(ctx, |e, ctx| {
            e.set_placeholder_text("e.g. npm install", ctx);
        });
        editor
    }

    fn open_action_form(&mut self, action_id: Option<Uuid>, ctx: &mut ViewContext<Self>) {
        self.panel_mode = PanelMode::EditAction(action_id);
        let config = WarpConfig::as_ref(ctx);
        let action = action_id.and_then(|id| config.actions().iter().find(|a| a.id == id).cloned());
        drop(config);

        let (name, desc, commands) = if let Some(a) = action {
            (a.name.clone(), a.description.clone().unwrap_or_default(), a.commands.clone())
        } else {
            (String::new(), String::new(), vec![String::new()])
        };

        self.edit_name_editor.update(ctx, |e, ctx| {
            e.set_buffer_text_with_base_buffer(&name, ctx);
        });
        self.edit_desc_editor.update(ctx, |e, ctx| {
            e.set_buffer_text_with_base_buffer(&desc, ctx);
        });

        // Build one editor per command (at least one empty row for new actions).
        self.edit_command_editors.clear();
        self.edit_command_remove_states.borrow_mut().clear();
        let cmds = if commands.is_empty() { vec![String::new()] } else { commands };
        for cmd_text in cmds {
            let row_id = Uuid::new_v4();
            let editor = self.make_command_editor(ctx);
            editor.update(ctx, |e, ctx| {
                e.set_buffer_text_with_base_buffer(&cmd_text, ctx);
            });
            self.edit_command_editors.push((row_id, editor));
        }
        ctx.notify();
    }

    fn open_trigger_form(&mut self, trigger_id: Option<Uuid>, ctx: &mut ViewContext<Self>) {
        self.panel_mode = PanelMode::EditTrigger(trigger_id);
        let config = WarpConfig::as_ref(ctx);
        let trigger = trigger_id
            .and_then(|id| config.triggers().iter().find(|t| t.id == id).cloned());
        drop(config);

        let (name, desc, ordered_ids) = if let Some(t) = trigger {
            (
                t.name.clone(),
                t.description.clone().unwrap_or_default(),
                t.action_ids.clone(),
            )
        } else {
            (String::new(), String::new(), Vec::new())
        };

        self.edit_selected_action_ids = ordered_ids;
        self.trigger_search_query = String::new();

        self.edit_name_editor.update(ctx, |e, ctx| {
            e.set_buffer_text_with_base_buffer(&name, ctx);
        });
        self.edit_desc_editor.update(ctx, |e, ctx| {
            e.set_buffer_text_with_base_buffer(&desc, ctx);
        });
        self.trigger_search_editor.update(ctx, |e, ctx| {
            e.set_buffer_text_with_base_buffer("", ctx);
        });
        ctx.notify();
    }

    fn open_workspace_name_form(&mut self, workspace_id: Option<Uuid>, ctx: &mut ViewContext<Self>) {
        self.panel_mode = PanelMode::EditWorkspaceName(workspace_id);
        let existing_name = workspace_id.and_then(|id| {
            let config = WarpConfig::as_ref(ctx);
            config.saved_workspaces().iter().find(|w| w.id == id).map(|w| w.name.clone())
        });
        let name = existing_name.unwrap_or_default();
        self.edit_workspace_name_editor.update(ctx, |e, ctx| {
            e.set_buffer_text_with_base_buffer(&name, ctx);
        });
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

    fn workspace_states(&self, id: Uuid) -> (MouseStateHandle, MouseStateHandle, MouseStateHandle) {
        let mut map = self.workspace_row_states.borrow_mut();
        let s = map.entry(id).or_insert_with(RowMouseStates::default);
        (s.primary.clone(), s.secondary.clone(), s.delete.clone())
    }

    fn action_toggle_state(&self, id: Uuid) -> MouseStateHandle {
        let mut map = self.edit_action_toggle_states.borrow_mut();
        map.entry(id).or_insert_with(MouseStateHandle::default).clone()
    }

    fn selected_move_up_state(&self, id: Uuid) -> MouseStateHandle {
        let mut map = self.edit_selected_move_up_states.borrow_mut();
        map.entry(id).or_insert_with(MouseStateHandle::default).clone()
    }

    fn selected_move_down_state(&self, id: Uuid) -> MouseStateHandle {
        let mut map = self.edit_selected_move_down_states.borrow_mut();
        map.entry(id).or_insert_with(MouseStateHandle::default).clone()
    }

    fn selected_remove_state(&self, id: Uuid) -> MouseStateHandle {
        let mut map = self.edit_selected_remove_states.borrow_mut();
        map.entry(id).or_insert_with(MouseStateHandle::default).clone()
    }

    // ── Header ────────────────────────────────────────────────────────────

    fn render_header(&self, appearance: &Appearance) -> Box<dyn Element> {
        // In editing mode show a back arrow + form title instead of tabs.
        let left_side: Box<dyn Element> = match &self.panel_mode {
            PanelMode::List => {
                let tab_row = Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(4.0)
                    .with_child(self.render_tab_button(appearance, "Actions", ActionsPanelTab::Actions))
                    .with_child(self.render_tab_button(appearance, "Triggers", ActionsPanelTab::Triggers))
                    .with_child(self.render_tab_button(appearance, "Workspaces", ActionsPanelTab::Workspaces))
                    .with_main_axis_size(MainAxisSize::Min)
                    .finish();
                Shrinkable::new(1.0, tab_row).finish()
            }
            PanelMode::EditAction(id) => {
                let title = if id.is_some() { "Edit Action" } else { "New Action" };
                Shrinkable::new(
                    1.0,
                    Text::new(title, appearance.ui_font_family(), 13.)
                        .with_style(Properties::default().weight(Weight::Semibold))
                        .with_color(
                            appearance.theme().main_text_color(appearance.theme().background()).into_solid(),
                        )
                        .finish(),
                )
                .finish()
            }
            PanelMode::EditTrigger(id) => {
                let title = if id.is_some() { "Edit Trigger" } else { "New Trigger" };
                Shrinkable::new(
                    1.0,
                    Text::new(title, appearance.ui_font_family(), 13.)
                        .with_style(Properties::default().weight(Weight::Semibold))
                        .with_color(
                            appearance.theme().main_text_color(appearance.theme().background()).into_solid(),
                        )
                        .finish(),
                )
                .finish()
            }
            PanelMode::EditWorkspaceName(id) => {
                let title = if id.is_some() { "Rename Workspace" } else { "Save Workspace" };
                Shrinkable::new(
                    1.0,
                    Text::new(title, appearance.ui_font_family(), 13.)
                        .with_style(Properties::default().weight(Weight::Semibold))
                        .with_color(
                            appearance.theme().main_text_color(appearance.theme().background()).into_solid(),
                        )
                        .finish(),
                )
                .finish()
            }
        };

        Container::new(
            ConstrainedBox::new(left_side)
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
        let weight = if is_active { Weight::Semibold } else { Weight::Normal };
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

    // ── Empty states ──────────────────────────────────────────────────────

    fn render_empty_state(
        &self,
        appearance: &Appearance,
        icon: icons::Icon,
        title: &'static str,
        subtitle: &'static str,
        create_action: ActionsPanelAction,
        create_mouse_state: MouseStateHandle,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let sub_color = theme.sub_text_color(theme.background()).into_solid();
        let main_color = theme.main_text_color(theme.background()).into_solid();

        let icon_el = ConstrainedBox::new(
            icon.to_warpui_icon(theme.sub_text_color(theme.background())).finish(),
        )
        .with_width(32.)
        .with_height(32.)
        .finish();

        let title_el = Text::new(title, appearance.ui_font_family(), 14.)
            .with_style(Properties::default().weight(Weight::Semibold))
            .with_color(main_color)
            .finish();

        let subtitle_el = Text::new(subtitle, appearance.ui_font_family(), 12.)
            .with_color(sub_color)
            .finish();

        let create_btn = {
            let ui_builder = appearance.ui_builder().clone();
            ui_builder
                .button(ButtonVariant::Secondary, create_mouse_state)
                .with_style(UiComponentStyles {
                    font_size: Some(BUTTON_FONT_SIZE),
                    padding: Some(Coords {
                        top: 6.,
                        bottom: 6.,
                        left: 14.,
                        right: 14.,
                    }),
                    ..Default::default()
                })
                .with_text_label(format!("+ Create {}", title.split_whitespace().next().unwrap_or(title)))
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(create_action.clone());
                })
                .with_cursor(Cursor::PointingHand)
                .finish()
        };

        Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_main_axis_size(MainAxisSize::Max)
            .with_child(
                Container::new(icon_el)
                    .with_margin_bottom(12.)
                    .finish(),
            )
            .with_child(
                Container::new(title_el)
                    .with_margin_bottom(6.)
                    .finish(),
            )
            .with_child(
                Container::new(subtitle_el)
                    .with_margin_bottom(20.)
                    .finish(),
            )
            .with_child(create_btn)
            .finish()
    }

    // ── List tabs ─────────────────────────────────────────────────────────

    fn render_actions_tab(&self, actions: &[Action], appearance: &Appearance) -> Box<dyn Element> {
        let new_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder.tool_tip("Create new action".to_string()).build().finish();
            icon_button(appearance, Icon::Plus, false, self.new_action_mouse_state.clone())
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
            let empty_state_mouse = {
                let mut map = self.action_row_states.borrow_mut();
                map.entry(Uuid::nil())
                    .or_insert_with(RowMouseStates::default)
                    .primary
                    .clone()
            };
            Align::new(self.render_empty_state(
                appearance,
                Icon::Lightning,
                "Action",
                "Automate terminal commands",
                ActionsPanelAction::NewAction,
                empty_state_mouse,
            ))
            .finish()
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

    fn render_triggers_tab(
        &self,
        triggers: &[Trigger],
        has_actions: bool,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        // Guard: can't create triggers without actions.
        if !has_actions {
            let theme = appearance.theme();
            let sub_color = theme.sub_text_color(theme.background()).into_solid();
            let main_color = theme.main_text_color(theme.background()).into_solid();
            let icon_el = ConstrainedBox::new(
                Icon::Workflow
                    .to_warpui_icon(theme.sub_text_color(theme.background()))
                    .finish(),
            )
            .with_width(32.)
            .with_height(32.)
            .finish();
            let no_actions_hint = Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_main_axis_alignment(MainAxisAlignment::Center)
                .with_main_axis_size(MainAxisSize::Max)
                .with_child(Container::new(icon_el).with_margin_bottom(12.).finish())
                .with_child(
                    Container::new(
                        Text::new("Trigger", appearance.ui_font_family(), 14.)
                            .with_style(Properties::default().weight(Weight::Semibold))
                            .with_color(main_color)
                            .finish(),
                    )
                    .with_margin_bottom(6.)
                    .finish(),
                )
                .with_child(
                    Text::new(
                        "Create an action first,\nthen build a trigger.",
                        appearance.ui_font_family(),
                        12.,
                    )
                    .with_color(sub_color)
                    .finish(),
                )
                .finish();
            return Align::new(no_actions_hint).finish();
        }

        let new_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder.tool_tip("Create new trigger".to_string()).build().finish();
            icon_button(appearance, Icon::Plus, false, self.new_trigger_mouse_state.clone())
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
            let empty_state_mouse = {
                let mut map = self.trigger_row_states.borrow_mut();
                map.entry(Uuid::nil())
                    .or_insert_with(RowMouseStates::default)
                    .primary
                    .clone()
            };
            Align::new(self.render_empty_state(
                appearance,
                Icon::Workflow,
                "Trigger",
                "Run actions across multiple terminals",
                ActionsPanelAction::NewTrigger,
                empty_state_mouse,
            ))
            .finish()
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
            icon_button(appearance, Icon::Plus, false, self.save_workspace_mouse_state.clone())
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
            let empty_state_mouse = {
                let mut map = self.workspace_row_states.borrow_mut();
                map.entry(Uuid::nil())
                    .or_insert_with(RowMouseStates::default)
                    .primary
                    .clone()
            };
            Align::new(self.render_empty_state(
                appearance,
                Icon::Folder,
                "Workspace",
                "Save and restore window layouts",
                ActionsPanelAction::SaveWorkspace,
                empty_state_mouse,
            ))
            .finish()
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

    // ── List rows ─────────────────────────────────────────────────────────

    fn render_action_row(&self, action: &Action, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let font = appearance.ui_font_family();

        let name = Text::new(action.name.clone(), font, 13.)
            .with_style(Properties::default().weight(Weight::Medium))
            .with_color(theme.main_text_color(theme.background()).into_solid())
            .finish();

        let action_id = action.id;
        let is_builtin = is_builtin_action(&action_id);
        let (run_state, edit_state, delete_state) = self.action_states(action_id);

        let meta_label = if is_builtin {
            "built-in".to_string()
        } else {
            format!(
                "{} command{}",
                action.commands.len(),
                if action.commands.len() == 1 { "" } else { "s" }
            )
        };
        let meta = Text::new(meta_label, font, 11.)
            .with_color(theme.sub_text_color(theme.background()).into_solid())
            .finish();

        let run_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder.tool_tip("Run in active terminal".to_string()).build().finish();
            icon_button(appearance, Icon::Play, false, run_state)
                .with_tooltip(move || tooltip)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::RunAction(action_id));
                })
                .finish()
        };

        let mut buttons_row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(2.0)
            .with_child(run_button);

        if !is_builtin {
            let edit_button = {
                let ui_builder = appearance.ui_builder().clone();
                let tooltip = ui_builder.tool_tip("Edit action".to_string()).build().finish();
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
                let tooltip = ui_builder.tool_tip("Delete action".to_string()).build().finish();
                let icon_color = theme.sub_text_color(theme.background());
                icon_button_with_color(appearance, Icon::Trash, false, delete_state, icon_color)
                    .with_tooltip(move || tooltip)
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(ActionsPanelAction::DeleteAction(action_id));
                    })
                    .finish()
            };
            buttons_row = buttons_row.with_child(edit_button).with_child(delete_button);
        }

        let buttons = buttons_row.with_main_axis_size(MainAxisSize::Min).finish();

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
            let tooltip = ui_builder.tool_tip("Run trigger across targets".to_string()).build().finish();
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
            let tooltip = ui_builder.tool_tip("Edit trigger".to_string()).build().finish();
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
            let tooltip = ui_builder.tool_tip("Delete trigger".to_string()).build().finish();
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
        let (restore_state, rename_state, delete_state) = self.workspace_states(workspace_id);

        let restore_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder.tool_tip("Restore workspace".to_string()).build().finish();
            icon_button(appearance, Icon::Refresh, false, restore_state)
                .with_tooltip(move || tooltip)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::RestoreWorkspace(workspace_id));
                })
                .finish()
        };

        let rename_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder.tool_tip("Rename workspace".to_string()).build().finish();
            let icon_color = theme.sub_text_color(theme.background());
            icon_button_with_color(appearance, Icon::Edit, false, rename_state, icon_color)
                .with_tooltip(move || tooltip)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::RenameWorkspace(workspace_id));
                })
                .finish()
        };

        let delete_button = {
            let ui_builder = appearance.ui_builder().clone();
            let tooltip = ui_builder.tool_tip("Delete workspace".to_string()).build().finish();
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
            .with_child(rename_button)
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

    // ── Editor forms ──────────────────────────────────────────────────────

    fn render_field_label(&self, label: &str, appearance: &Appearance) -> Box<dyn Element> {
        Container::new(
            Text::new(label.to_string(), appearance.ui_font_family(), LABEL_SIZE)
                .with_style(Properties::default().weight(Weight::Semibold))
                .with_color(
                    appearance
                        .theme()
                        .sub_text_color(appearance.theme().background())
                        .into_solid(),
                )
                .finish(),
        )
        .with_margin_bottom(4.)
        .finish()
    }

    fn render_text_field(
        &self,
        editor: &ViewHandle<EditorView>,
        height: Option<f32>,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let input = appearance
            .ui_builder()
            .text_input(editor.clone())
            .with_style(UiComponentStyles {
                padding: Some(Coords::uniform(8.)),
                background: Some(blended_colors::neutral_2(theme).into()),
                border_radius: Some(CornerRadius::with_all(Radius::Pixels(4.))),
                border_color: Some(theme.sub_text_color(theme.background()).into()),
                ..Default::default()
            })
            .build()
            .finish();

        let mut cb = ConstrainedBox::new(input);
        if let Some(h) = height {
            cb = cb.with_height(h);
        }
        Container::new(cb.finish()).with_margin_bottom(FIELD_SPACING).finish()
    }

    fn render_form_buttons(&self, appearance: &Appearance) -> Box<dyn Element> {
        let cancel_btn = appearance
            .ui_builder()
            .button(ButtonVariant::Secondary, self.cancel_form_state.clone())
            .with_style(UiComponentStyles {
                font_size: Some(BUTTON_FONT_SIZE),
                padding: Some(Coords { top: 7., bottom: 7., left: 14., right: 14. }),
                ..Default::default()
            })
            .with_text_label("Cancel".to_string())
            .build()
            .on_click(|ctx, _, _| ctx.dispatch_typed_action(ActionsPanelAction::CancelForm))
            .with_cursor(Cursor::PointingHand)
            .finish();

        let save_btn = appearance
            .ui_builder()
            .button(ButtonVariant::Accent, self.save_form_state.clone())
            .with_style(UiComponentStyles {
                font_size: Some(BUTTON_FONT_SIZE),
                padding: Some(Coords { top: 7., bottom: 7., left: 14., right: 14. }),
                ..Default::default()
            })
            .with_text_label("Save".to_string())
            .build()
            .on_click(|ctx, _, _| ctx.dispatch_typed_action(ActionsPanelAction::SaveForm))
            .with_cursor(Cursor::PointingHand)
            .finish();

        Container::new(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_main_axis_alignment(MainAxisAlignment::End)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_spacing(8.)
                .with_child(cancel_btn)
                .with_child(save_btn)
                .finish(),
        )
        .with_margin_top(4.)
        .finish()
    }

    fn render_command_remove_state(&self, row_id: Uuid) -> MouseStateHandle {
        self.edit_command_remove_states
            .borrow_mut()
            .entry(row_id)
            .or_insert_with(MouseStateHandle::default)
            .clone()
    }

    fn render_action_editor(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let font = appearance.ui_font_family();
        let sub_fill = theme.sub_text_color(theme.background());
        let sub_color = sub_fill.into_solid();

        let mut col = Flex::column().with_main_axis_size(MainAxisSize::Max);

        col = col.with_child(self.render_field_label("NAME", appearance));
        col = col.with_child(self.render_text_field(&self.edit_name_editor, Some(FIELD_HEIGHT), appearance));

        col = col.with_child(self.render_field_label("DESCRIPTION", appearance));
        col = col.with_child(self.render_text_field(&self.edit_desc_editor, Some(FIELD_HEIGHT), appearance));

        // ── Commands list ─────────────────────────────────────────────────
        col = col.with_child(self.render_field_label("COMMANDS", appearance));

        let total = self.edit_command_editors.len();
        for (pos, (row_id, editor_handle)) in self.edit_command_editors.iter().enumerate() {
            let row_id = *row_id;
            let remove_state = self.render_command_remove_state(row_id);

            // Numbered index label
            let index_label = ConstrainedBox::new(
                Text::new(format!("{}", pos + 1), font, 11.)
                    .with_color(sub_color)
                    .finish(),
            )
            .with_width(16.)
            .finish();

            // Single-line text field for the command
            let input = self.render_text_field(editor_handle, Some(FIELD_HEIGHT), appearance);

            // Delete button (hidden when only one row remains)
            let delete_btn: Box<dyn Element> = if total > 1 {
                icon_button_with_color(appearance, Icon::X, false, remove_state, sub_fill)
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(ActionsPanelAction::RemoveCommandRow(row_id));
                    })
                    .with_cursor(Cursor::PointingHand)
                    .finish()
            } else {
                // Invisible spacer to keep alignment consistent
                ConstrainedBox::new(Container::new(Flex::row().finish()).finish())
                    .with_width(22.)
                    .finish()
            };

            let row = Container::new(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(4.)
                    .with_child(index_label)
                    .with_child(Shrinkable::new(1.0, input).finish())
                    .with_child(delete_btn)
                    .with_main_axis_size(MainAxisSize::Max)
                    .finish(),
            )
            .with_margin_bottom(4.)
            .finish();

            col = col.with_child(row);
        }

        // "+ Add Command" button
        let add_btn = appearance
            .ui_builder()
            .button(ButtonVariant::Secondary, self.add_command_state.clone())
            .with_style(UiComponentStyles {
                font_size: Some(12.),
                padding: Some(Coords { top: 5., bottom: 5., left: 10., right: 10. }),
                ..Default::default()
            })
            .with_text_label("+ Add Command".to_string())
            .build()
            .on_click(|ctx, _, _| ctx.dispatch_typed_action(ActionsPanelAction::AddCommandRow))
            .with_cursor(Cursor::PointingHand)
            .finish();

        col = col.with_child(
            Container::new(add_btn)
                .with_margin_bottom(FIELD_SPACING)
                .finish(),
        );

        col = col.with_child(self.render_form_buttons(appearance));

        Container::new(col.finish())
            .with_padding_left(FORM_PADDING)
            .with_padding_right(FORM_PADDING)
            .with_padding_top(FORM_PADDING)
            .with_padding_bottom(FORM_PADDING)
            .finish()
    }

    fn render_workspace_name_editor(&self, appearance: &Appearance) -> Box<dyn Element> {
        let mut col = Flex::column().with_main_axis_size(MainAxisSize::Max);
        col = col.with_child(self.render_field_label("NAME", appearance));
        col = col.with_child(
            self.render_text_field(&self.edit_workspace_name_editor, Some(FIELD_HEIGHT), appearance),
        );
        col = col.with_child(self.render_form_buttons(appearance));
        Container::new(col.finish())
            .with_padding_left(FORM_PADDING)
            .with_padding_right(FORM_PADDING)
            .with_padding_top(FORM_PADDING)
            .with_padding_bottom(FORM_PADDING)
            .finish()
    }

    fn render_trigger_editor(
        &self,
        actions: &[Action],
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let font = appearance.ui_font_family();
        let sub_color = theme.sub_text_color(theme.background()).into_solid();
        let sub_fill = theme.sub_text_color(theme.background());
        let main_color = theme.main_text_color(theme.background()).into_solid();

        let mut col = Flex::column().with_main_axis_size(MainAxisSize::Max);

        col = col.with_child(self.render_field_label("NAME", appearance));
        col = col.with_child(self.render_text_field(&self.edit_name_editor, Some(FIELD_HEIGHT), appearance));

        col = col.with_child(self.render_field_label("DESCRIPTION", appearance));
        col = col.with_child(self.render_text_field(&self.edit_desc_editor, Some(FIELD_HEIGHT), appearance));

        // ── Selected actions (ordered list) ───────────────────────────────
        if !self.edit_selected_action_ids.is_empty() {
            col = col.with_child(self.render_field_label("SELECTED ACTIONS  (in execution order)", appearance));

            for (pos, &action_id) in self.edit_selected_action_ids.iter().enumerate() {
                let action_name = actions
                    .iter()
                    .find(|a| a.id == action_id)
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| "(unknown)".to_string());

                let total = self.edit_selected_action_ids.len();
                let up_state = self.selected_move_up_state(action_id);
                let down_state = self.selected_move_down_state(action_id);
                let remove_state = self.selected_remove_state(action_id);

                let name_el = Text::new(action_name, font, 12.)
                    .with_color(main_color)
                    .finish();

                let up_btn = icon_button_with_color(appearance, Icon::ArrowUp, false, up_state, sub_fill)
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(ActionsPanelAction::MoveActionUp(action_id));
                    })
                    .finish();

                let down_btn = icon_button_with_color(appearance, Icon::ArrowDown, false, down_state, sub_fill)
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(ActionsPanelAction::MoveActionDown(action_id));
                    })
                    .finish();

                let remove_btn = icon_button_with_color(appearance, Icon::X, false, remove_state, sub_fill)
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(ActionsPanelAction::RemoveActionFromTrigger(action_id));
                    })
                    .finish();

                // Dim up/down buttons at boundaries — still rendered but visually subdued.
                let order_label = Text::new(
                    format!("{}.", pos + 1),
                    font,
                    11.,
                )
                .with_color(sub_color)
                .finish();

                let mut controls = Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(2.);
                if pos > 0 {
                    controls = controls.with_child(up_btn);
                }
                if pos + 1 < total {
                    controls = controls.with_child(down_btn);
                }
                controls = controls.with_child(remove_btn);

                let row = Container::new(
                    Flex::row()
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                        .with_main_axis_size(MainAxisSize::Max)
                        .with_child(
                            Flex::row()
                                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                                .with_spacing(6.)
                                .with_child(order_label)
                                .with_child(name_el)
                                .finish(),
                        )
                        .with_child(controls.with_main_axis_size(MainAxisSize::Min).finish())
                        .finish(),
                )
                .with_padding_top(4.)
                .with_padding_bottom(4.)
                .finish();

                col = col.with_child(row);
            }

            col = col.with_child(
                Container::new(
                    Text::new(
                        "—",
                        font,
                        11.,
                    )
                    .with_color(sub_color)
                    .finish(),
                )
                .with_margin_top(4.)
                .with_margin_bottom(8.)
                .finish(),
            );
        }

        // ── Action search/picker ──────────────────────────────────────────
        col = col.with_child(self.render_field_label("ADD ACTIONS", appearance));

        // Search field
        col = col.with_child(self.render_text_field(&self.trigger_search_editor, Some(FIELD_HEIGHT), appearance));

        let query = self.trigger_search_query.to_lowercase();
        let available: Vec<&Action> = actions
            .iter()
            .filter(|a| {
                !self.edit_selected_action_ids.contains(&a.id)
                    && (query.is_empty() || a.name.to_lowercase().contains(&query))
            })
            .collect();

        if available.is_empty() {
            col = col.with_child(
                Container::new(
                    Text::new(
                        if query.is_empty() {
                            "All actions are already added."
                        } else {
                            "No matching actions."
                        },
                        font,
                        12.,
                    )
                    .with_color(sub_color)
                    .finish(),
                )
                .with_margin_bottom(FIELD_SPACING)
                .finish(),
            );
        } else {
            for action in &available {
                let action_id = action.id;
                let toggle_state = self.action_toggle_state(action_id);

                let plus_icon = ConstrainedBox::new(
                    Icon::PlusCircle
                        .to_warpui_icon(theme.accent())
                        .finish(),
                )
                .with_width(14.)
                .with_height(14.)
                .finish();

                let name_el = Text::new(action.name.clone(), font, 12.)
                    .with_color(main_color)
                    .finish();

                let cmd_count = format!(
                    "{} cmd{}",
                    action.commands.len(),
                    if action.commands.len() == 1 { "" } else { "s" }
                );
                let meta_el = Text::new(cmd_count, font, 11.)
                    .with_color(sub_color)
                    .finish();

                let row = Hoverable::new(toggle_state, move |state| {
                    let bg = if state.is_hovered() {
                        Some(warpui::elements::Fill::Solid(
                            pathfinder_color::ColorU::new(255, 255, 255, 10),
                        ))
                    } else {
                        None
                    };
                    let mut c = Container::new(
                        Flex::row()
                            .with_cross_axis_alignment(CrossAxisAlignment::Center)
                            .with_main_axis_size(MainAxisSize::Max)
                            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                            .with_child(
                                Flex::row()
                                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                                    .with_spacing(8.)
                                    .with_child(plus_icon)
                                    .with_child(name_el)
                                    .finish(),
                            )
                            .with_child(meta_el)
                            .finish(),
                    )
                    .with_padding_top(6.)
                    .with_padding_bottom(6.);
                    if let Some(bg) = bg {
                        c = c.with_background(bg);
                    }
                    c.finish()
                })
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(ActionsPanelAction::ToggleActionInTrigger(action_id));
                })
                .with_cursor(Cursor::PointingHand)
                .finish();

                col = col.with_child(row);
            }
        }

        col = col.with_child(self.render_form_buttons(appearance));

        Container::new(Shrinkable::new(1.0, col.finish()).finish())
            .with_padding_left(FORM_PADDING)
            .with_padding_right(FORM_PADDING)
            .with_padding_top(FORM_PADDING)
            .with_padding_bottom(FORM_PADDING)
            .finish()
    }
}

// ── Action enum ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum ActionsPanelAction {
    SetTab(ActionsPanelTab),
    ClosePanel,
    RunAction(Uuid),
    RunTrigger(Uuid),
    SaveWorkspace,
    RenameWorkspace(Uuid),
    RestoreWorkspace(Uuid),
    DeleteWorkspace(Uuid),
    NewAction,
    NewTrigger,
    DeleteAction(Uuid),
    DeleteTrigger(Uuid),
    EditAction(Uuid),
    EditTrigger(Uuid),
    ToggleActionInTrigger(Uuid),
    MoveActionUp(Uuid),
    MoveActionDown(Uuid),
    RemoveActionFromTrigger(Uuid),
    /// Append a new empty command row to the action editor.
    AddCommandRow,
    /// Remove the command row with the given stable UUID.
    RemoveCommandRow(Uuid),
    SaveForm,
    CancelForm,
}

// ── View impl ─────────────────────────────────────────────────────────────────

impl warpui::Entity for ActionsPanelView {
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

        let content: Box<dyn Element> = match &self.panel_mode {
            PanelMode::List => match self.active_tab {
                ActionsPanelTab::Actions => self.render_actions_tab(&actions, appearance),
                ActionsPanelTab::Triggers => {
                    self.render_triggers_tab(&triggers, !actions.is_empty(), appearance)
                }
                ActionsPanelTab::Workspaces => self.render_workspaces_tab(&workspaces, appearance),
            },
            PanelMode::EditAction(_) => self.render_action_editor(appearance),
            PanelMode::EditTrigger(_) => self.render_trigger_editor(&actions, appearance),
            PanelMode::EditWorkspaceName(_) => self.render_workspace_name_editor(appearance),
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
            .on_resize(move |ctx, _| ctx.notify())
            .with_bounds_callback(Box::new(|window_size| {
                let min_width = MIN_SIDEBAR_WIDTH;
                let max_width = window_size.x() * MAX_SIDEBAR_WIDTH_RATIO;
                (min_width, max_width.max(min_width))
            }))
            .finish()
    }
}

// ── TypedActionView impl ──────────────────────────────────────────────────────

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

            // ── Run ────────────────────────────────────────────────────────
            ActionsPanelAction::RunAction(id) => {
                if crate::actions::model::is_builtin_action(id) {
                    // Built-in actions dispatch directly as WorkspaceActions.
                    if *id == crate::actions::model::BUILTIN_CLOSE_ALL_TERMINALS_ID {
                        ctx.dispatch_typed_action(&WorkspaceAction::CloseAllTerminals);
                    } else if *id == crate::actions::model::BUILTIN_KILL_ALL_PROCESSES_ID {
                        ctx.dispatch_typed_action(&WorkspaceAction::KillAllTerminalProcesses);
                    }
                } else {
                    ctx.dispatch_typed_action(&WorkspaceAction::RunActionInActiveTerminal(*id));
                }
            }
            ActionsPanelAction::RunTrigger(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::RunTrigger(*id));
            }

            // ── Workspaces ─────────────────────────────────────────────────
            ActionsPanelAction::SaveWorkspace => {
                self.active_tab = ActionsPanelTab::Workspaces;
                self.open_workspace_name_form(None, ctx);
            }
            ActionsPanelAction::RenameWorkspace(id) => {
                self.open_workspace_name_form(Some(*id), ctx);
            }
            ActionsPanelAction::RestoreWorkspace(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::RestoreWorkspace(*id));
            }
            ActionsPanelAction::DeleteWorkspace(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::DeleteWorkspace(*id));
            }

            // ── Delete ─────────────────────────────────────────────────────
            ActionsPanelAction::DeleteAction(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::DeleteAction(*id));
            }
            ActionsPanelAction::DeleteTrigger(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::DeleteTrigger(*id));
            }

            // ── Open editor forms ──────────────────────────────────────────
            ActionsPanelAction::NewAction => {
                self.active_tab = ActionsPanelTab::Actions;
                self.open_action_form(None, ctx);
            }
            ActionsPanelAction::EditAction(id) => {
                self.open_action_form(Some(*id), ctx);
            }
            ActionsPanelAction::NewTrigger => {
                let has_actions = {
                    let config = WarpConfig::as_ref(ctx);
                    !config.actions().is_empty()
                };
                if !has_actions {
                    return;
                }
                self.active_tab = ActionsPanelTab::Triggers;
                self.open_trigger_form(None, ctx);
            }
            ActionsPanelAction::EditTrigger(id) => {
                self.open_trigger_form(Some(*id), ctx);
            }

            // ── Trigger action selector ────────────────────────────────────
            ActionsPanelAction::ToggleActionInTrigger(action_id) => {
                if !self.edit_selected_action_ids.contains(action_id) {
                    self.edit_selected_action_ids.push(*action_id);
                    ctx.notify();
                }
            }
            ActionsPanelAction::MoveActionUp(action_id) => {
                if let Some(pos) = self.edit_selected_action_ids.iter().position(|id| id == action_id) {
                    if pos > 0 {
                        self.edit_selected_action_ids.swap(pos, pos - 1);
                        ctx.notify();
                    }
                }
            }
            ActionsPanelAction::MoveActionDown(action_id) => {
                if let Some(pos) = self.edit_selected_action_ids.iter().position(|id| id == action_id) {
                    if pos + 1 < self.edit_selected_action_ids.len() {
                        self.edit_selected_action_ids.swap(pos, pos + 1);
                        ctx.notify();
                    }
                }
            }
            ActionsPanelAction::RemoveActionFromTrigger(action_id) => {
                self.edit_selected_action_ids.retain(|id| id != action_id);
                ctx.notify();
            }

            // ── Save / Cancel form ─────────────────────────────────────────
            ActionsPanelAction::AddCommandRow => {
                let row_id = Uuid::new_v4();
                let editor = self.make_command_editor(ctx);
                self.edit_command_editors.push((row_id, editor));
                ctx.notify();
            }
            ActionsPanelAction::RemoveCommandRow(row_id) => {
                // Always keep at least one row so the form is never empty.
                if self.edit_command_editors.len() > 1 {
                    self.edit_command_editors.retain(|(id, _)| id != row_id);
                    self.edit_command_remove_states.borrow_mut().remove(row_id);
                    ctx.notify();
                }
            }
            ActionsPanelAction::CancelForm => {
                self.panel_mode = PanelMode::List;
                ctx.notify();
            }
            ActionsPanelAction::SaveForm => {
                // Workspace name form uses its own editor; handle it first.
                if let PanelMode::EditWorkspaceName(maybe_id) = self.panel_mode.clone() {
                    let ws_name = self
                        .edit_workspace_name_editor
                        .read(ctx, |e, ctx| e.buffer_text(ctx))
                        .trim()
                        .to_string();
                    if ws_name.is_empty() {
                        return;
                    }
                    match maybe_id {
                        None => {
                            ctx.dispatch_typed_action(&WorkspaceAction::SaveCurrentWorkspaceWithName(
                                ws_name,
                            ));
                        }
                        Some(id) => {
                            ctx.dispatch_typed_action(&WorkspaceAction::RenameWorkspace(id, ws_name));
                        }
                    }
                    self.panel_mode = PanelMode::List;
                    ctx.notify();
                    return;
                }

                let name = self
                    .edit_name_editor
                    .read(ctx, |e, ctx| e.buffer_text(ctx))
                    .trim()
                    .to_string();
                if name.is_empty() {
                    return;
                }
                let desc_raw = self
                    .edit_desc_editor
                    .read(ctx, |e, ctx| e.buffer_text(ctx))
                    .trim()
                    .to_string();
                let description = if desc_raw.is_empty() { None } else { Some(desc_raw) };

                match self.panel_mode.clone() {
                    PanelMode::EditAction(maybe_id) => {
                        // Collect non-empty commands from the per-row editors.
                        let commands: Vec<String> = self
                            .edit_command_editors
                            .iter()
                            .map(|(_, handle)| handle.read(ctx, |e, ctx| e.buffer_text(ctx)).trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();

                        let config = WarpConfig::as_ref(ctx);
                        let existing_source = maybe_id
                            .and_then(|id| config.actions().iter().find(|a| a.id == id).cloned())
                            .and_then(|a| a.source_path.clone());
                        drop(config);

                        // Delete old file if editing and name changed.
                        if let Some(old_path) = existing_source.as_ref() {
                            let _ = std::fs::remove_file(old_path);
                        }

                        let new_action = super::model::Action {
                            id: maybe_id.unwrap_or_else(Uuid::new_v4),
                            name,
                            description,
                            commands,
                            source_path: None,
                        };
                        let action_clone = new_action.clone();
                        if let Err(e) = storage::write_action(&new_action) {
                            log::error!("Failed to save action: {e}");
                            return;
                        }
                        let is_new = maybe_id.is_none();
                        WarpConfig::handle(ctx).update(ctx, move |config, ctx| {
                            if is_new {
                                config.add_action(action_clone, ctx);
                            } else {
                                config.update_action(action_clone, ctx);
                            }
                        });
                    }
                    PanelMode::EditTrigger(maybe_id) => {
                        let selected = self.edit_selected_action_ids.iter().cloned().collect::<Vec<_>>();

                        let config = WarpConfig::as_ref(ctx);
                        let existing_source = maybe_id
                            .and_then(|id| config.triggers().iter().find(|t| t.id == id).cloned())
                            .and_then(|t| t.source_path.clone());
                        drop(config);

                        if let Some(old_path) = existing_source.as_ref() {
                            let _ = std::fs::remove_file(old_path);
                        }

                        let new_trigger = super::model::Trigger {
                            id: maybe_id.unwrap_or_else(Uuid::new_v4),
                            name,
                            description,
                            action_ids: selected,
                            targets: Default::default(),
                            source_path: None,
                        };
                        let trigger_clone = new_trigger.clone();
                        if let Err(e) = storage::write_trigger(&new_trigger) {
                            log::error!("Failed to save trigger: {e}");
                            return;
                        }
                        let is_new = maybe_id.is_none();
                        WarpConfig::handle(ctx).update(ctx, move |config, ctx| {
                            if is_new {
                                config.add_trigger(trigger_clone, ctx);
                            } else {
                                config.update_trigger(trigger_clone, ctx);
                            }
                        });
                    }
                    PanelMode::List | PanelMode::EditWorkspaceName(_) => {}
                }

                self.panel_mode = PanelMode::List;
                ctx.notify();
            }
        }
    }
}
