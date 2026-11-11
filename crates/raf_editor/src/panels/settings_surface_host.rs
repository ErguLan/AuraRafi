//! Native host/controller for engine settings.

use raf_core::ai::AiProvider;
use raf_core::config::EngineSettings;
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
    UiDispatchedAction,
};
use raf_render::api_graphic_basic::EditorUiLayer;

use crate::editor_layout::EditorRect;
use crate::settings_surface::{
    account_controls, account_welcome, build_settings_surface_with_state_and_api_keys, provider_id, SettingsSection,
};
use raf_ui::numeric::{UiNumericDraftIssue, UiNumericEditAction, UiNumericEditBank, UiNumericUnit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSurfaceIntent {
    Changed,
    Close,
}

pub struct SettingsSurfaceHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    pub draft: EngineSettings,
    section: SettingsSection,
    draft_dirty: bool,
    surface_dirty: bool,
    last_rect: Option<EditorRect>,
    modal_rect: raf_ui::UiRect,
    drag_anchor: Option<[f32; 2]>,
    resize_anchor: Option<([f32; 2], raf_ui::UiRect)>,
    search_query: String,
    open_select: Option<String>,
    revealed_api_keys: Vec<AiProvider>,
    ai_model_filter: String,
    catalog_fingerprint: String,
    catalog_loading: bool,
    numeric_edits: UiNumericEditBank,
    account_view: raf_ai::chatgpt_auth::AuthView,
    activate_plan_after_auth: Option<u64>,
}

const MODAL_MARGIN: f32 = 24.0;
const MODAL_BOUNDS_MARGIN: f32 = 16.0;
const MODAL_MIN_WIDTH: f32 = 720.0;
const MODAL_MIN_HEIGHT: f32 = 520.0;
const MODAL_DEFAULT_MAX_WIDTH: f32 = 1080.0;
const MODAL_DEFAULT_MAX_HEIGHT: f32 = 720.0;
const MODAL_MAX_WIDTH: f32 = 1440.0;
const MODAL_MAX_HEIGHT: f32 = 960.0;

fn default_modal_rect(rect: EditorRect) -> raf_ui::UiRect {
    let viewport_width = rect.width.max(1.0);
    let viewport_height = rect.height.max(1.0);
    let available_width = (viewport_width - MODAL_MARGIN * 2.0).max(1.0);
    let available_height = (viewport_height - MODAL_MARGIN * 2.0).max(1.0);
    let width = available_width
        .min(MODAL_DEFAULT_MAX_WIDTH)
        .max(MODAL_MIN_WIDTH.min(available_width));
    let height = available_height
        .min(MODAL_DEFAULT_MAX_HEIGHT)
        .max(MODAL_MIN_HEIGHT.min(available_height));
    raf_ui::UiRect::new(
        ((viewport_width - width) * 0.5).max(0.0),
        ((viewport_height - height) * 0.5).max(0.0),
        width,
        height,
    )
}

fn modal_bounds(rect: EditorRect) -> raf_ui::UiRect {
    raf_ui::UiRect::new(
        MODAL_BOUNDS_MARGIN,
        MODAL_BOUNDS_MARGIN,
        (rect.width - MODAL_BOUNDS_MARGIN * 2.0).max(1.0),
        (rect.height - MODAL_BOUNDS_MARGIN * 2.0).max(1.0),
    )
}

fn clamp_modal_rect(modal: raf_ui::UiRect, rect: EditorRect) -> raf_ui::UiRect {
    let bounds = modal_bounds(rect);
    let min_width = MODAL_MIN_WIDTH.min(bounds.width);
    let min_height = MODAL_MIN_HEIGHT.min(bounds.height);
    let max_width = MODAL_MAX_WIDTH.min(bounds.width).max(min_width);
    let max_height = MODAL_MAX_HEIGHT.min(bounds.height).max(min_height);
    let width = modal.width.clamp(min_width, max_width);
    let height = modal.height.clamp(min_height, max_height);
    raf_ui::UiRect::new(modal.x, modal.y, width, height).clamp_inside(bounds)
}

fn resize_modal_rect(
    start: raf_ui::UiRect,
    origin: [f32; 2],
    point: [f32; 2],
    rect: EditorRect,
) -> raf_ui::UiRect {
    clamp_modal_rect(
        raf_ui::UiRect::new(
            start.x,
            start.y,
            start.width + point[0] - origin[0],
            start.height + point[1] - origin[1],
        ),
        rect,
    )
}

impl SettingsSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        let draft = EngineSettings::default();
        let modal_rect = default_modal_rect(rect);
        let mut host = Self {
            region: InputRegionId::from_static("native.editor.settings"),
            rect,
            host: graphics.create_ui_host(
                build_settings_surface_with_state_and_api_keys(
                    palette,
                    &draft,
                    SettingsSection::Appearance,
                    modal_rect,
                    "",
                    None,
                    &[],
                    "",
                ),
                [0, 0, 0, 0],
            ),
            draft,
            section: SettingsSection::Appearance,
            draft_dirty: false,
            surface_dirty: true,
            last_rect: None,
            modal_rect,
            drag_anchor: None,
            resize_anchor: None,
            search_query: String::new(),
            open_select: None,
            revealed_api_keys: Vec::new(),
            ai_model_filter: String::new(),
            catalog_fingerprint: String::new(),
            catalog_loading: false,
            numeric_edits: UiNumericEditBank::default(),
            account_view: raf_ai::chatgpt_auth::AuthView::default(),
            activate_plan_after_auth: None,
        };
        host.refresh_numeric_bindings();
        host.seed_numeric_inputs();
        host
    }

    pub fn owner(&self) -> InputOwner {
        InputOwner::Modal(self.region)
    }

    pub fn reset_input_state(&mut self, router: &mut InputRouter) {
        self.host
            .session_mut()
            .reset_interaction_for_surface_change(None);
        router.cancel_owner(self.owner());
        self.drag_anchor = None;
        self.resize_anchor = None;
        self.open_select = None;
        self.revealed_api_keys.clear();
        self.ai_model_filter.clear();
        self.host
            .session_mut()
            .interaction
            .focus
            .request_focus("settings.search");
    }

    pub fn reset_draft(&mut self, settings: &EngineSettings) {
        self.draft = settings.clone();
        self.draft_dirty = false;
        self.drag_anchor = None;
        self.resize_anchor = None;
        self.search_query.clear();
        self.open_select = None;
        self.revealed_api_keys.clear();
        self.ai_model_filter.clear();
        self.numeric_edits.cancel_all();
        self.refresh_numeric_bindings();
        self.surface_dirty = true;
    }

    /// Restores editable preferences to their engine defaults. Window
    /// geometry is intentionally preserved because it is shell state, not a
    /// setting the modal currently exposes. The user can still cancel before
    /// applying the reset.
    pub fn restore_defaults(&mut self) {
        let window_width = self.draft.window_width;
        let window_height = self.draft.window_height;
        let window_maximized = self.draft.window_maximized;
        self.draft = EngineSettings::default();
        self.draft.window_width = window_width;
        self.draft.window_height = window_height;
        self.draft.window_maximized = window_maximized;
        self.draft_dirty = true;
        self.drag_anchor = None;
        self.resize_anchor = None;
        self.search_query.clear();
        self.open_select = None;
        self.revealed_api_keys.clear();
        self.ai_model_filter.clear();
        self.numeric_edits.cancel_all();
        self.refresh_numeric_bindings();
        self.surface_dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.draft_dirty
    }

    pub fn section(&self) -> SettingsSection {
        self.section
    }

    pub fn set_section(&mut self, section: SettingsSection) {
        if self.section != section {
            self.section = section;
            self.open_select = None;
            self.surface_dirty = true;
        }
    }

    pub fn modal_rect(&self) -> raf_ui::UiRect {
        self.modal_rect
    }

    pub fn cursor_hint(&self) -> raf_ui::UiCursorIcon {
        self.host.cursor_hint()
    }

    pub fn has_interactive_hover(&self) -> bool {
        self.host.has_interactive_hover()
    }

    pub fn has_active_text_repeat(&self) -> bool {
        self.host.has_active_text_repeat() || self.account_view.busy || self.catalog_loading
    }

    pub fn account_command(&mut self, name: &str) -> bool {
        let activates_plan = name == "settings.account.connect"
            || name.starts_with("settings.account.reconnect:") || name.starts_with("settings.account.select:");
        let result = if name == "settings.account.connect" {
            raf_ai::chatgpt_auth::begin_sign_in(None, self.draft.language)
        } else if let Some(index) = name.strip_prefix("settings.account.reconnect:").and_then(|v| v.parse::<usize>().ok()) {
            raf_ai::chatgpt_auth::begin_sign_in(Some(index), self.draft.language)
        } else if let Some(index) = name.strip_prefix("settings.account.select:").and_then(|v| v.parse::<usize>().ok()) {
            raf_ai::chatgpt_auth::select_account(index).and_then(|()| raf_ai::chatgpt_auth::begin_model_refresh())
        } else if name == "settings.account.refresh" {
            raf_ai::chatgpt_auth::begin_model_refresh()
        } else if name == "settings.account.signout" {
            raf_ai::chatgpt_auth::begin_sign_out()
        } else if name == "settings.account.welcome_ack" {
            raf_ai::chatgpt_auth::acknowledge_welcome()
        } else if name == "settings.account.usage" {
            if crate::script_support::open_url("https://chatgpt.com/settings/usage") { Ok(()) }
            else { Err("Could not open account usage settings.".into()) }
        } else { return false; };
        if result.is_ok() && activates_plan { self.activate_plan_after_auth = Some(self.account_view.generation); }
        if let Some(view) = raf_ai::chatgpt_auth::snapshot() { self.account_view = view; }
        if let Err(error) = result { self.account_view.message = Some(error); }
        self.surface_dirty = true;
        true
    }

    pub fn set_select_open(&mut self, id: &str, open: bool) {
        self.open_select = open.then(|| id.to_string());
        self.surface_dirty = true;
    }

    pub fn toggle_api_key(&mut self, provider: AiProvider) {
        if let Some(index) = self
            .revealed_api_keys
            .iter()
            .position(|candidate| *candidate == provider)
        {
            self.revealed_api_keys.remove(index);
        } else {
            self.revealed_api_keys.push(provider);
        }
        self.surface_dirty = true;
    }

    pub fn set_search_query(&mut self, query: impl Into<String>) {
        let query = query.into();
        if self.search_query == query {
            return;
        }
        self.search_query = query;
        if !self.search_query.trim().is_empty() {
            if let Some(section) = SettingsSection::ALL
                .into_iter()
                .find(|section| section.matches_query(&self.search_query))
            {
                self.section = section;
            }
        }
        self.open_select = None;
        self.surface_dirty = true;
    }

    pub fn mark_dirty(&mut self) {
        self.draft_dirty = true;
        self.surface_dirty = true;
    }

    pub fn mark_draft_dirty(&mut self) {
        self.draft_dirty = true;
    }

    pub fn commit_numeric_drafts(&mut self) -> bool {
        let before = self.draft.clone();
        self.try_commit_numeric_drafts() && self.draft != before
    }

    /// Validates every enabled numeric draft first, then applies all of them
    /// to a cloned settings value so invalid fields cannot partially mutate
    /// the modal draft.
    pub fn try_commit_numeric_drafts(&mut self) -> bool {
        self.refresh_numeric_bindings();
        let values = match self.numeric_edits.pending_values() {
            Ok(values) => values,
            Err(issues) => {
                self.mark_numeric_issues(&issues);
                return false;
            }
        };

        let mut next_draft = self.draft.clone();
        for value in &values {
            if !crate::native_workbench::apply_settings_range(
                &mut next_draft,
                &value.key,
                value.value,
            ) {
                self.numeric_edits.mark_rejected(&value.key);
                self.set_numeric_invalid(&value.key, true);
                return false;
            }
        }

        if let Err(issues) = self.numeric_edits.commit_pending() {
            self.mark_numeric_issues(&issues);
            return false;
        }

        if self.draft != next_draft {
            self.draft = next_draft;
            self.draft_dirty = true;
        }
        self.surface_dirty = true;
        true
    }

    /// Read-only validation hook for callers that need to guard Save/Apply.
    pub fn numeric_drafts_valid(&self) -> bool {
        let Ok(values) = self.numeric_edits.pending_values() else {
            return false;
        };
        let mut candidate = self.draft.clone();
        values.iter().all(|value| {
            crate::native_workbench::apply_settings_range(&mut candidate, &value.key, value.value)
        })
    }

    pub fn is_numeric_text_key(&self, key: &str) -> bool {
        self.numeric_edits.binding_for_text_key(key).is_some()
    }

    fn mark_numeric_issues(&mut self, issues: &[UiNumericDraftIssue]) {
        for issue in issues {
            self.numeric_edits.mark_rejected(&issue.key);
            self.set_numeric_invalid(&issue.key, true);
        }
    }

    fn refresh_numeric_bindings(&mut self) {
        let palette = self.host.surface().palette;
        for section in SettingsSection::ALL {
            let surface = build_settings_surface_with_state_and_api_keys(
                palette,
                &self.draft,
                section,
                self.modal_rect,
                "",
                None,
                &self.revealed_api_keys,
                &self.ai_model_filter,
            );
            self.numeric_edits.merge_tree(&surface.root);
        }
        // Runtime width is authored in pixels; the range remains the source
        // of truth for its numeric bounds.
        self.numeric_edits
            .set_unit("settings.runtime.width", UiNumericUnit::Pixels);
    }

    fn set_numeric_invalid(&mut self, key: &str, invalid: bool) {
        let Some(node_id) = self
            .numeric_edits
            .bindings()
            .find(|binding| binding.key == key)
            .map(|binding| binding.node_id.clone())
        else {
            return;
        };
        if self
            .host
            .surface()
            .root
            .find(&node_id)
            .is_some_and(|node| node.invalid != invalid)
        {
            if let Some(node) = self.host.surface_mut().root.find_mut(&node_id) {
                node.invalid = invalid;
            }
        }
    }

    fn sync_numeric_invalid_markers(&mut self) {
        let states = self
            .numeric_edits
            .bindings()
            .map(|binding| {
                (
                    binding.node_id.clone(),
                    self.numeric_edits.is_invalid(&binding.key),
                )
            })
            .collect::<Vec<_>>();
        for (node_id, invalid) in states {
            if self
                .host
                .surface()
                .root
                .find(&node_id)
                .is_some_and(|node| node.invalid != invalid)
            {
                if let Some(node) = self.host.surface_mut().root.find_mut(&node_id) {
                    node.invalid = invalid;
                }
            }
        }
    }

    pub fn accept(&mut self) -> EngineSettings {
        self.draft_dirty = false;
        self.drag_anchor = None;
        self.resize_anchor = None;
        self.surface_dirty = true;
        self.draft.clone()
    }

    pub fn cancel(&mut self, committed: &EngineSettings) {
        self.draft = committed.clone();
        self.numeric_edits.cancel_all();
        self.refresh_numeric_bindings();
        self.draft_dirty = false;
        self.drag_anchor = None;
        self.resize_anchor = None;
        self.revealed_api_keys.clear();
        self.ai_model_filter.clear();
        self.surface_dirty = true;
    }

    pub fn text(&self, key: &str) -> String {
        self.host
            .session()
            .interaction
            .controls
            .text(key)
            .to_string()
    }

    pub fn focused_text_rect(&self) -> Option<raf_ui::UiRect> {
        let rect = self.host.focused_text_rect()?;
        Some(raf_ui::UiRect::new(
            rect.x + self.rect.x,
            rect.y + self.rect.y,
            rect.width,
            rect.height,
        ))
    }

    pub fn set_environment(&mut self, environment: raf_ui::UiEnvironment) {
        self.host.set_environment(environment);
    }

    pub fn sync(
        &mut self,
        palette: StudioUiPalette,
        settings: &EngineSettings,
        section: SettingsSection,
        rect: EditorRect,
    ) {
        self.catalog_loading = false;
        if section == SettingsSection::Ai {
            if self.draft.agent_openai_use_plan { raf_ai::chatgpt_auth::ensure_models_requested(); }
            if let Some(view) = raf_ai::chatgpt_auth::snapshot() {
                if view != self.account_view {
                    if self.activate_plan_after_auth.is_some_and(|generation| generation != view.generation)
                        && !view.busy && view.accounts.iter().any(|a| a.active && a.connected) {
                        self.draft.agent_openai_use_plan = true;
                        self.draft.default_ai_provider = AiProvider::OpenAI;
                        if let Some(provider) = self.draft.ai_providers.iter_mut().find(|p| p.provider == AiProvider::OpenAI) { provider.enabled = true; }
                        self.draft_dirty = true;
                    }
                    if !view.busy { self.activate_plan_after_auth = None; }
                    // Discovery never silently changes a user's selected model.
                    self.account_view = view;
                    self.surface_dirty = true;
                }
            }
            // The displayed provider card refreshes its model catalog on
            // demand; the fingerprint turns a finished fetch into a surface
            // rebuild so rows appear without reopening the picker.
            if let Some(provider) =
                self.draft
                    .ai_providers
                    .iter()
                    .find(|config| config.provider == self.draft.default_ai_provider)
                    .cloned()
            {
                if !(provider.provider == AiProvider::OpenAI && self.draft.agent_openai_use_plan) && matches!(
                    provider.provider,
                    AiProvider::OpenRouter
                        | AiProvider::OpenAI
                        | AiProvider::OpenCode
                        | AiProvider::OpenCodeGo
                ) {
                    let base_url = match provider.provider {
                        AiProvider::OpenRouter | AiProvider::OpenAI => provider.base_url.clone(),
                        _ => String::new(),
                    };
                    let trigger = format!("settings.ai-provider.{}.model.trigger", provider_id(provider.provider));
                    if self.open_select.as_deref() == Some(trigger.as_str()) {
                        raf_ai::provider_catalog::ensure_requested(provider.provider, &base_url, &provider.api_key);
                    }
                    let view = raf_ai::provider_catalog::snapshot_for(provider.provider, &base_url, &provider.api_key);
                    self.catalog_loading = view.loading;
                    let fingerprint = format!("{:?}{:?}{}{}", view.models, view.error, view.loading, view.fetched);
                    if fingerprint != self.catalog_fingerprint {
                        self.catalog_fingerprint = fingerprint;
                        self.surface_dirty = true;
                    }
                }
            }
        }
        if !self.draft_dirty && self.draft != *settings {
            self.draft = settings.clone();
            self.refresh_numeric_bindings();
            self.surface_dirty = true;
        }
        if self.section != section {
            self.section = section;
            self.surface_dirty = true;
        }
        if self.last_rect != Some(rect) {
            self.modal_rect = clamp_modal_rect(self.modal_rect, rect);
            self.surface_dirty = true;
        }
        self.rect = rect;
        if self.surface_dirty {
            let mut surface = build_settings_surface_with_state_and_api_keys(
                palette,
                &self.draft,
                self.section,
                self.modal_rect,
                &self.search_query,
                self.open_select.as_deref(),
                &self.revealed_api_keys,
                &self.ai_model_filter,
            );
            if self.section == SettingsSection::Ai {
                // The ChatGPT account block is reachable as soon as the AI
                // section opens: the auth button must not depend on which
                // provider card is currently selected.
                let account = account_controls(
                    palette,
                    &self.draft,
                    &self.account_view,
                    self.open_select.as_deref(),
                );
                if let Some(card) = surface.root.find_mut("settings.ai-provider.openai") {
                    card.children.insert(2.min(card.children.len()), account);
                } else if let Some(section_card) = surface.root.find_mut("settings.ai_providers") {
                    section_card
                        .children
                        .insert(3.min(section_card.children.len()), account);
                }
                if self.account_view.needs_welcome {
                    surface.root.children.push(account_welcome(palette, self.modal_rect));
                }
            }
            self.numeric_edits.merge_tree(&surface.root);
            self.host.set_surface(surface);
            self.seed_numeric_inputs();
            self.sync_numeric_invalid_markers();
            self.last_rect = Some(rect);
            self.surface_dirty = false;
        }
    }

    fn seed_numeric_inputs(&mut self) {
        let numeric_values = self
            .numeric_edits
            .bindings()
            .filter_map(|binding| {
                self.numeric_edits
                    .draft_text(&binding.key)
                    .map(|text| (binding.text_key.clone(), text.to_string()))
            })
            .collect::<Vec<_>>();
        let values = [
            (
                "settings.script_external_editor.text",
                self.draft.script_external_editor_cmd.clone(),
            ),
            (
                "settings.script_external_editor",
                self.draft.script_external_editor_cmd.clone(),
            ),
            ("settings.search", self.search_query.clone()),
            ("settings.ai.model.filter", self.ai_model_filter.clone()),
        ];
        let ai_values = raf_core::ai::AiProvider::all()
            .iter()
            .filter_map(|provider| {
                let id = provider_id(*provider);
                let config = self
                    .draft
                    .ai_providers
                    .iter()
                    .find(|config| config.provider == *provider)?;
                Some([
                    (
                        format!("settings.ai_provider.{id}.base_url"),
                        config.base_url.clone(),
                    ),
                    (
                        format!("settings.ai_provider.{id}.model"),
                        config.model.clone(),
                    ),
                    (
                        format!("settings.ai_provider.{id}.api_key"),
                        config.api_key.clone(),
                    ),
                ])
            })
            .flatten()
            .collect::<Vec<_>>();
        let controls = &mut self.host.session_mut().interaction.controls;
        for (key, value) in numeric_values {
            controls.set_text(key, value, 32);
        }
        for (key, value) in values {
            let max_length = match key {
                "settings.search" => 256,
                "settings.script_external_editor.text" | "settings.script_external_editor" => 256,
                "settings.ai.model.filter" => 256,
                _ => 32,
            };
            controls.set_text(key, value, max_length);
        }
        for (key, value) in ai_values {
            controls.set_text(key, value, 512);
        }
        for (field, value) in [
            (
                "repository_url",
                &self.draft.external_asset_studio.repository_url,
            ),
            ("directory", &self.draft.external_asset_studio.directory),
            ("program", &self.draft.external_asset_studio.program),
            ("entry_point", &self.draft.external_asset_studio.entry_point),
        ] {
            controls.set_text(
                format!("settings.external_studio.{field}"),
                value.clone(),
                512,
            );
        }
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) -> Vec<UiDispatchedAction> {
        let language = self.draft.language;
        let actions = self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| raf_core::i18n::t(key, language),
            input,
            router,
            InputOwner::Modal(self.region),
            raf_ui::UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        );

        let mut returned_actions = Vec::with_capacity(actions.len() + 1);
        for dispatched in actions.iter().cloned() {
            match &dispatched.action {
                // The model picker search box is host-owned presentation
                // state; it never reaches the settings draft.
                UiAction::SetText { key, value }
                    if key == "settings.ai.model.filter" =>
                {
                    let value = value.clone();
                    if self.ai_model_filter != value {
                        self.ai_model_filter = value;
                        self.surface_dirty = true;
                    }
                }
                UiAction::SetText { key, value }
                    if self.numeric_edits.binding_for_text_key(key).is_some() =>
                {
                    let numeric_key = self
                        .numeric_edits
                        .binding_for_text_key(key)
                        .map(|binding| binding.key.clone())
                        .expect("numeric binding was checked above");
                    self.numeric_edits.edit_text(key, value.clone());
                    self.draft_dirty = true;
                    let invalid = self.numeric_edits.is_invalid(&numeric_key);
                    self.set_numeric_invalid(&numeric_key, invalid);
                    returned_actions.push(dispatched);
                }
                UiAction::SetRange { key, value }
                    if self
                        .numeric_edits
                        .bindings()
                        .any(|binding| binding.key.as_str() == key.as_str()) =>
                {
                    self.numeric_edits.cancel(key);
                    let text = self
                        .numeric_edits
                        .format_value(key, *value)
                        .unwrap_or_else(|| value.to_string());
                    self.numeric_edits.sync_committed(key, *value, text.clone());
                    if let Some(binding) = self
                        .numeric_edits
                        .bindings()
                        .find(|binding| binding.key.as_str() == key.as_str())
                    {
                        self.host.session_mut().interaction.controls.set_text(
                            binding.text_key.clone(),
                            text,
                            32,
                        );
                    }
                    self.set_numeric_invalid(key, false);
                    returned_actions.push(dispatched);
                }
                UiAction::Command { name }
                    if self
                        .numeric_edits
                        .binding_for_submit_command(name)
                        .is_some() =>
                {
                    let numeric_key = self
                        .numeric_edits
                        .binding_for_submit_command(name)
                        .map(|binding| binding.key.clone())
                        .expect("numeric binding was checked above");
                    match self.numeric_edits.commit_for_submit(name) {
                        Some(UiNumericEditAction::Committed { key, value }) => {
                            let text = self
                                .numeric_edits
                                .format_value(&key, value)
                                .unwrap_or_else(|| value.to_string());
                            self.numeric_edits.sync_committed(&key, value, text.clone());
                            if let Some(text_key) = self
                                .numeric_edits
                                .bindings()
                                .find(|binding| binding.key.as_str() == key.as_str())
                                .map(|binding| binding.text_key.clone())
                            {
                                self.host
                                    .session_mut()
                                    .interaction
                                    .controls
                                    .set_text(text_key, text, 32);
                            }
                            self.set_numeric_invalid(&key, false);
                            returned_actions.push(UiDispatchedAction {
                                target_id: dispatched.target_id.clone(),
                                event: dispatched.event.clone(),
                                action: UiAction::SetRange { key, value },
                            });
                        }
                        Some(UiNumericEditAction::Rejected { .. }) | None => {
                            self.set_numeric_invalid(&numeric_key, true);
                        }
                        _ => {}
                    }
                }
                UiAction::Command { name } if name == "settings.numeric.cancel" => {
                    let numeric_key = self
                        .numeric_edits
                        .binding_for_node(&dispatched.target_id)
                        .map(|binding| binding.key.clone());
                    if let Some(numeric_key) = numeric_key {
                        if let Some(UiNumericEditAction::Cancelled { restored_text, .. }) =
                            self.numeric_edits.cancel(&numeric_key)
                        {
                            let text_key = self
                                .numeric_edits
                                .bindings()
                                .find(|binding| binding.key.as_str() == numeric_key.as_str())
                                .map(|binding| binding.text_key.clone());
                            if let Some(text_key) = text_key {
                                self.host.session_mut().interaction.controls.set_text(
                                    text_key,
                                    restored_text,
                                    32,
                                );
                            }
                            self.set_numeric_invalid(&numeric_key, false);
                            returned_actions.push(UiDispatchedAction {
                                target_id: dispatched.target_id.clone(),
                                event: dispatched.event.clone(),
                                action: UiAction::Command {
                                    name: "settings.numeric.cancelled".to_string(),
                                },
                            });
                        }
                    }
                }
                UiAction::Command { name }
                    if matches!(name.as_str(), "settings.apply" | "settings.save") =>
                {
                    returned_actions.push(dispatched);
                }
                _ => returned_actions.push(dispatched),
            }
        }

        let pointer = self.host.session().interaction.pointer_position();
        for dispatched in &returned_actions {
            let UiAction::Command { name } = &dispatched.action else {
                continue;
            };
            match name.as_str() {
                "settings.modal.drag.start" => {
                    self.resize_anchor = None;
                    self.drag_anchor = pointer
                        .map(|point| [point[0] - self.modal_rect.x, point[1] - self.modal_rect.y]);
                }
                "settings.modal.drag.move" => {
                    if let (Some(anchor), Some(point)) = (self.drag_anchor, pointer) {
                        let next = clamp_modal_rect(
                            raf_ui::UiRect::new(
                                point[0] - anchor[0],
                                point[1] - anchor[1],
                                self.modal_rect.width,
                                self.modal_rect.height,
                            ),
                            self.rect,
                        );
                        if next != self.modal_rect {
                            self.modal_rect = next;
                            self.surface_dirty = true;
                        }
                    }
                }
                "settings.modal.drag.end" => self.drag_anchor = None,
                "settings.modal.resize.start" => {
                    self.drag_anchor = None;
                    self.resize_anchor = pointer.map(|point| (point, self.modal_rect));
                }
                "settings.modal.resize.move" => {
                    if let (Some((origin, start)), Some(point)) = (self.resize_anchor, pointer) {
                        let next = resize_modal_rect(start, origin, point, self.rect);
                        if next != self.modal_rect {
                            self.modal_rect = next;
                            self.surface_dirty = true;
                        }
                    }
                }
                "settings.modal.resize.end" => self.resize_anchor = None,
                _ => {}
            }
        }
        returned_actions
    }

    pub fn compositor_layer(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> EditorUiLayer<'_> {
        EditorUiLayer {
            host: &mut self.host,
            target_rect: self.rect.to_physical(scale_factor, target_size),
            logical_size: self.rect.logical_size(),
            raster_scale: scale_factor.max(1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_modal_fits_the_editor_with_safe_margins() {
        let viewport = EditorRect::new(0.0, 0.0, 1280.0, 800.0);
        let modal = default_modal_rect(viewport);

        assert_eq!(modal.width, MODAL_DEFAULT_MAX_WIDTH);
        assert_eq!(modal.height, MODAL_DEFAULT_MAX_HEIGHT);
        assert!(modal.x >= MODAL_MARGIN);
        assert!(modal.y >= MODAL_MARGIN);
        assert!(modal.right() <= viewport.width);
        assert!(modal.bottom() <= viewport.height);
    }

    #[test]
    fn narrow_editor_keeps_the_modal_inside_the_viewport() {
        let viewport = EditorRect::new(0.0, 0.0, 767.0, 643.0);
        let modal = default_modal_rect(viewport);

        assert!(modal.x >= 0.0);
        assert!(modal.y >= 0.0);
        assert!(modal.right() <= viewport.width);
        assert!(modal.bottom() <= viewport.height);
        assert!(modal.width < MODAL_MIN_WIDTH);
        assert!(modal.height > MODAL_MIN_HEIGHT);
    }

    #[test]
    fn resize_grows_from_the_bottom_right_and_respects_limits() {
        let viewport = EditorRect::new(0.0, 0.0, 1600.0, 1000.0);
        let start = raf_ui::UiRect::new(120.0, 80.0, 720.0, 520.0);

        let grown = resize_modal_rect(start, [840.0, 600.0], [1080.0, 760.0], viewport);
        assert_eq!(grown.x, start.x);
        assert_eq!(grown.y, start.y);
        assert_eq!(grown.width, 960.0);
        assert_eq!(grown.height, 680.0);

        let clamped = resize_modal_rect(start, [840.0, 600.0], [5000.0, 5000.0], viewport);
        assert!(clamped.width <= MODAL_MAX_WIDTH);
        assert!(clamped.height <= MODAL_MAX_HEIGHT);
        assert!(clamped.right() <= viewport.width - MODAL_BOUNDS_MARGIN);
        assert!(clamped.bottom() <= viewport.height - MODAL_BOUNDS_MARGIN);
    }
}
