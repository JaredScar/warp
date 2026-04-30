use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use uuid::Uuid;
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
use crate::editor::{EditorOptions, EditorView, SingleLineEditorOptions, TextOptions};
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
const COMMANDS_HEIGHT: f32 = 120.;
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
    close_button_mouse_state: MouseStateHandle,
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
    edit_commands_editor: ViewHandle<EditorView>,
    /// Action IDs currently selected in the trigger editor.
    edit_selected_action_ids: HashSet<Uuid>,
    /// Stable toggle-button states for each action in the trigger editor (keyed by UUID).
    edit_action_toggle_states: RefCell<HashMap<Uuid, MouseStateHandle>>,
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
            ctx.add_typed_action_view(|ctx| EditorView::single_line(single_line_opts, ctx));

        edit_name_editor.update(ctx, |editor, ctx| {
            editor.set_placeholder_text("Name (required)", ctx);
        });
        edit_desc_editor.update(ctx, |editor, ctx| {
            editor.set_placeholder_text("Description (optional)", ctx);
        });

        let multi_opts = EditorOptions {
            autogrow: false,
            soft_wrap: true,
            text: TextOptions {
                font_size_override: Some(font_size),
                ..Default::default()
            },
            ..Default::default()
        };
        let edit_commands_editor =
            ctx.add_typed_action_view(|ctx| EditorView::new(multi_opts, ctx));
        edit_commands_editor.update(ctx, |editor, ctx| {
            editor.set_placeholder_text("One command per line\ne.g. git status\n     npm install", ctx);
        });

        Self {
            resizable_state_handle: resizable_state_handle(360.0),
            close_button_mouse_state: Default::default(),
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
            edit_commands_editor,
            edit_selected_action_ids: Default::default(),
            edit_action_toggle_states: Default::default(),
            save_form_state: Default::default(),
            cancel_form_state: Default::default(),
        }
    }

    pub fn set_active_tab(&mut self, tab: ActionsPanelTab, ctx: &mut ViewContext<Self>) {
        self.active_tab = tab;
        ctx.notify();
    }

    // ── Form open/populate ────────────────────────────────────────────────

    fn open_action_form(&mut self, action_id: Option<Uuid>, ctx: &mut ViewContext<Self>) {
        self.panel_mode = PanelMode::EditAction(action_id);
        let config = WarpConfig::as_ref(ctx);
        let action = action_id.and_then(|id| config.actions().iter().find(|a| a.id == id).cloned());
        drop(config);

        let (name, desc, commands) = if let Some(a) = action {
            let cmds = a.commands.join("\n");
            (a.name.clone(), a.description.clone().unwrap_or_default(), cmds)
        } else {
            (String::new(), String::new(), String::new())
        };

        self.edit_name_editor.update(ctx, |e, ctx| {
            e.clear_buffer_and_reset_undo_stack(ctx);
            if !name.is_empty() {
                e.set_base_buffer_text(name, ctx);
            }
        });
        self.edit_desc_editor.update(ctx, |e, ctx| {
            e.clear_buffer_and_reset_undo_stack(ctx);
            if !desc.is_empty() {
                e.set_base_buffer_text(desc, ctx);
            }
        });
        self.edit_commands_editor.update(ctx, |e, ctx| {
            e.clear_buffer_and_reset_undo_stack(ctx);
            if !commands.is_empty() {
                e.set_base_buffer_text(commands, ctx);
            }
        });
        ctx.notify();
    }

    fn open_trigger_form(&mut self, trigger_id: Option<Uuid>, ctx: &mut ViewContext<Self>) {
        self.panel_mode = PanelMode::EditTrigger(trigger_id);
        let config = WarpConfig::as_ref(ctx);
        let trigger = trigger_id
            .and_then(|id| config.triggers().iter().find(|t| t.id == id).cloned());
        drop(config);

        let (name, desc, selected_ids) = if let Some(t) = trigger {
            (
                t.name.clone(),
                t.description.clone().unwrap_or_default(),
                t.action_ids.iter().cloned().collect::<HashSet<_>>(),
            )
        } else {
            (String::new(), String::new(), HashSet::new())
        };

        self.edit_selected_action_ids = selected_ids;

        self.edit_name_editor.update(ctx, |e, ctx| {
            e.clear_buffer_and_reset_undo_stack(ctx);
            if !name.is_empty() {
                e.set_base_buffer_text(name, ctx);
            }
        });
        self.edit_desc_editor.update(ctx, |e, ctx| {
            e.clear_buffer_and_reset_undo_stack(ctx);
            if !desc.is_empty() {
                e.set_base_buffer_text(desc, ctx);
            }
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

    fn workspace_states(&self, id: Uuid) -> (MouseStateHandle, MouseStateHandle) {
        let mut map = self.workspace_row_states.borrow_mut();
        let s = map.entry(id).or_insert_with(RowMouseStates::default);
        (s.primary.clone(), s.delete.clone())
    }

    fn action_toggle_state(&self, id: Uuid) -> MouseStateHandle {
        let mut map = self.edit_action_toggle_states.borrow_mut();
        map.entry(id).or_insert_with(MouseStateHandle::default).clone()
    }

    // ── Header ────────────────────────────────────────────────────────────

    fn render_header(&self, appearance: &Appearance) -> Box<dyn Element> {
        let icon_color = appearance
            .theme()
            .sub_text_color(appearance.theme().background());

        let close_button = {
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
        };

        Container::new(
            ConstrainedBox::new(
                Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(left_side)
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
            let empty_state_mouse = {
                let mut map = self.trigger_row_states.borrow_mut();
                map.entry(Uuid::nil())
                    .or_insert_with(RowMouseStates::default)
                    .primary
                    .clone()
            };
            // Reuse the empty state layout but with a "create actions first" message.
            // We render the icon + text but disable the button.
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
            let tooltip = ui_builder.tool_tip("Run in active terminal".to_string()).build().finish();
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
        let (restore_state, delete_state) = self.workspace_states(workspace_id);

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

    fn render_action_editor(&self, appearance: &Appearance) -> Box<dyn Element> {
        let mut col = Flex::column().with_main_axis_size(MainAxisSize::Max);

        col = col.with_child(self.render_field_label("NAME", appearance));
        col = col.with_child(self.render_text_field(&self.edit_name_editor, Some(FIELD_HEIGHT), appearance));

        col = col.with_child(self.render_field_label("DESCRIPTION", appearance));
        col = col.with_child(self.render_text_field(&self.edit_desc_editor, Some(FIELD_HEIGHT), appearance));

        col = col.with_child(self.render_field_label("COMMANDS  (one per line)", appearance));
        col = col.with_child(self.render_text_field(&self.edit_commands_editor, Some(COMMANDS_HEIGHT), appearance));

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

        let mut col = Flex::column().with_main_axis_size(MainAxisSize::Max);

        col = col.with_child(self.render_field_label("NAME", appearance));
        col = col.with_child(self.render_text_field(&self.edit_name_editor, Some(FIELD_HEIGHT), appearance));

        col = col.with_child(self.render_field_label("DESCRIPTION", appearance));
        col = col.with_child(self.render_text_field(&self.edit_desc_editor, Some(FIELD_HEIGHT), appearance));

        col = col.with_child(self.render_field_label("ACTIONS  (select to include)", appearance));

        if actions.is_empty() {
            col = col.with_child(
                Container::new(
                    Text::new("No actions created yet.", font, 12.)
                        .with_color(theme.sub_text_color(theme.background()).into_solid())
                        .finish(),
                )
                .with_margin_bottom(FIELD_SPACING)
                .finish(),
            );
        } else {
            for action in actions {
                let action_id = action.id;
                let is_selected = self.edit_selected_action_ids.contains(&action_id);
                let toggle_state = self.action_toggle_state(action_id);
                let name_color = theme.main_text_color(theme.background()).into_solid();

                let check_color = if is_selected { theme.accent() } else { theme.sub_text_color(theme.background()) };

                let check_icon = ConstrainedBox::new(
                    (if is_selected { Icon::Check } else { Icon::Circle })
                        .to_warpui_icon(check_color)
                        .finish(),
                )
                .with_width(14.)
                .with_height(14.)
                .finish();

                let name = Text::new(action.name.clone(), font, 12.)
                    .with_color(name_color)
                    .finish();

                let row = Hoverable::new(toggle_state, move |_| {
                    Container::new(
                        Flex::row()
                            .with_cross_axis_alignment(CrossAxisAlignment::Center)
                            .with_spacing(8.)
                            .with_child(check_icon)
                            .with_child(name)
                            .finish(),
                    )
                    .with_padding_top(6.)
                    .with_padding_bottom(6.)
                    .finish()
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
    RestoreWorkspace(Uuid),
    DeleteWorkspace(Uuid),
    NewAction,
    NewTrigger,
    DeleteAction(Uuid),
    DeleteTrigger(Uuid),
    EditAction(Uuid),
    EditTrigger(Uuid),
    ToggleActionInTrigger(Uuid),
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
                ctx.dispatch_typed_action(&WorkspaceAction::RunActionInActiveTerminal(*id));
            }
            ActionsPanelAction::RunTrigger(id) => {
                ctx.dispatch_typed_action(&WorkspaceAction::RunTrigger(*id));
            }

            // ── Workspaces ─────────────────────────────────────────────────
            ActionsPanelAction::SaveWorkspace => {
                ctx.dispatch_typed_action(&WorkspaceAction::SaveCurrentWorkspace);
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
                if self.edit_selected_action_ids.contains(action_id) {
                    self.edit_selected_action_ids.remove(action_id);
                } else {
                    self.edit_selected_action_ids.insert(*action_id);
                }
                ctx.notify();
            }

            // ── Save / Cancel form ─────────────────────────────────────────
            ActionsPanelAction::CancelForm => {
                self.panel_mode = PanelMode::List;
                ctx.notify();
            }
            ActionsPanelAction::SaveForm => {
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
                        let commands_text = self
                            .edit_commands_editor
                            .read(ctx, |e, ctx| e.buffer_text(ctx));
                        let commands: Vec<String> = commands_text
                            .lines()
                            .map(|l| l.trim().to_string())
                            .filter(|l| !l.is_empty())
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
                    PanelMode::List => {}
                }

                self.panel_mode = PanelMode::List;
                ctx.notify();
            }
        }
    }
}
