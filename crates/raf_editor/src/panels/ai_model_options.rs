//! Shared model picker content for the Settings provider card and the
//! Agent panel menu.
//!
//! One data model, one row anatomy. The provider card is the single source of
//! truth for the active model (`AiProviderConfig.model`), pinned favorites
//! live on that card, and the fetched or curated catalogs feed suggestions.

use raf_core::ai::AiProvider;
use raf_core::config::EngineSettings;
use raf_core::{i18n, Language};
use raf_ui::geometry::UiSpacing;
use raf_ui::layout::{UiAlign, UiFlow, UiLayout, UiSizeMode};
use raf_ui::node::{UiAccessibilityRole, UiNode, UiNodeKind};
use raf_ui::text::{UiTextOverflow, UiTextStyle};
use raf_ui::{UiAction, UiEventBinding, UiEventKind};

/// One selectable model entry in a picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelItem {
    /// Ordered value sent to the action channel (`account:{slug}` selects an
    /// OpenAI plan model, raw ids select card models).
    pub value: String,
    /// Raw id used for pinning and deduplication.
    pub id: String,
    /// Human label; the raw id when no friendlier name exists.
    pub label: String,
    /// Part of the provider card favorites.
    pub pinned: bool,
    /// Currently active model on the card.
    pub selected: bool,
    /// Manually typed id that no longer appears in any catalog lane.
    pub custom: bool,
    pub available: bool,
}

/// Summary states rendered as headers or status lines in a picker popup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PickerCatalog {
    pub loading: bool,
    pub error: Option<String>,
    pub fetched: usize,
    pub interleaved_curated: usize,
    pub plan: usize,
    pub authoritative: bool,
}

pub struct ModelRowPaint {
    pub text: [u8; 4],
    pub muted: [u8; 4],
    pub accent_hot: [u8; 4],
}

pub struct ModelRowActions {
    /// Retained node id prefix for rows and pin toggles; hosts rely on it for
    /// the outside-click dismissal contract.
    pub node_prefix: String,
    /// `UiAction::SetSelect` key for row activation.
    pub select_key: String,
    /// Command prefix for the pin toggle; the ordered value is appended.
    pub pin_command_prefix: String,
    /// When present, activation also emits a `SetSelectOpen(open=false)` for
    /// this popup id so the menu closes immediately after selection.
    pub close_select_id: Option<String>,
}

/// The OpenAI plan owns the card model when the account toggle is on.
fn plan_owns_model(settings: &EngineSettings, provider: AiProvider) -> bool {
    provider == AiProvider::OpenAI && settings.agent_openai_use_plan
}

/// Builds the ordered, filtered picker items for one provider card.
///
/// Lanes: active model, pinned favorites, the merged fetched/curated catalog,
/// and the OpenAI account plan lane when it applies. The plan lane keeps the
/// account source separated by an `account:` value prefix, matching the
/// runtime adapter contract.
pub fn collect_items(
    settings: &EngineSettings,
    provider: AiProvider,
    query: &str,
) -> (PickerCatalog, Vec<ModelItem>) {
    let Some(config) = settings
        .ai_providers
        .iter()
        .find(|config| config.provider == provider)
    else {
        return (PickerCatalog::default(), Vec::new());
    };
    let account = if plan_owns_model(settings, provider) {
        raf_ai::chatgpt_auth::snapshot().unwrap_or_default()
    } else {
        raf_ai::chatgpt_auth::AuthView::default()
    };
    collect_from(
        config,
        plan_owns_model(settings, provider),
        &account,
        &raf_ai::provider_catalog::snapshot_for(provider, &config.base_url, &config.api_key),
        query,
    )
}

/// Kept pure so account/API separation can be tested without credentials.
fn collect_from(
    config: &raf_core::ai::AiProviderConfig,
    plan_owned: bool,
    account: &raf_ai::chatgpt_auth::AuthView,
    catalog: &raf_ai::provider_catalog::Catalog,
    query: &str,
) -> (PickerCatalog, Vec<ModelItem>) {
    let active = config.model.trim();
    let filter = query.trim().to_lowercase();
    let matches = |id: &str, label: &str| {
        filter.is_empty()
            || id.to_lowercase().contains(&filter)
            || label.to_lowercase().contains(&filter)
    };
    let mut items = Vec::new();
    if plan_owned {
        let connected = account.accounts.iter().any(|a| a.active && a.connected);
        let models = if connected {
            account.models.as_slice()
        } else {
            &[]
        };
        for model in models {
            if !matches(&model.slug, &model.label) {
                continue;
            }
            items.push(ModelItem {
                value: format!("account:{}", model.slug),
                id: model.slug.clone(),
                label: model.label.clone(),
                pinned: config.pinned_models.contains(&model.slug),
                selected: model.slug == active,
                custom: false,
                available: true,
            });
        }
        if !active.is_empty() && !models.iter().any(|m| m.slug == active) && matches(active, active)
        {
            items.push(ModelItem {
                value: format!("account:{active}"),
                id: active.into(),
                label: active.into(),
                pinned: false,
                selected: true,
                custom: true,
                available: false,
            });
        }
        return (
            PickerCatalog {
                loading: account.busy,
                plan: models.len(),
                error: if !connected {
                    Some("catalog_needs_account".into())
                } else if account.message.is_some() {
                    Some("catalog_unavailable".into())
                } else {
                    None
                },
                authoritative: connected && !account.busy && account.message.is_none(),
                ..PickerCatalog::default()
            },
            items,
        );
    }
    let suggestions: Vec<String> = if catalog.fetched {
        catalog.models.clone()
    } else {
        raf_ai::curated_models::curated(config.provider)
            .iter()
            .map(|id| (*id).into())
            .collect()
    };
    let available = |id: &str| {
        raf_ai::provider_catalog::suggested_model(config.provider, id)
            && (!catalog.fetched || suggestions.iter().any(|known| known == id))
    };
    let mut add = |id: &str, pinned: bool, custom: bool| {
        if matches(id, id) && !items.iter().any(|m: &ModelItem| m.id == id) {
            items.push(ModelItem {
                value: id.into(),
                id: id.into(),
                label: id.into(),
                pinned,
                selected: id == active,
                custom,
                available: available(id),
            });
        }
    };
    if !active.is_empty() && !suggestions.iter().any(|id| id == active) {
        add(
            active,
            config.pinned_models.iter().any(|id| id == active),
            true,
        );
    }
    for id in &config.pinned_models {
        add(id, true, false);
    }
    for id in &suggestions {
        add(id, false, false);
    }
    (
        PickerCatalog {
            loading: catalog.loading,
            error: catalog.error.clone(),
            fetched: if catalog.fetched {
                suggestions.len()
            } else {
                0
            },
            interleaved_curated: if catalog.fetched {
                0
            } else {
                suggestions.len()
            },
            authoritative: catalog.fetched,
            plan: 0,
        },
        items,
    )
}

/// Ordered values must match the current credential lane, including favorites.
pub fn selection_allowed(settings: &EngineSettings, provider: AiProvider, value: &str) -> bool {
    collect_items(settings, provider, "")
        .1
        .iter()
        .any(|item| item.value == value && item.available)
}

/// Compact header row used inside model pickers.
pub fn model_header_row(paint: &ModelRowPaint, key: &str, language: Language) -> UiNode {
    UiNode::new(format!("model-picker.header.{key}"), UiNodeKind::Label)
        .with_layout(
            UiLayout::fixed(0.0, 24.0)
                .with_width_mode(UiSizeMode::Fill)
                .padding_xy(10.0, 4.0),
        )
        .with_text_value(i18n::t(key, language))
        .with_text_style(UiTextStyle::button(paint.muted))
}

/// Status line for the catalog lane (loading state or a translated recovery
/// hint). Empty when there is nothing to say.
pub fn model_status_text(state: &PickerCatalog, language: Language) -> String {
    if state.loading {
        return i18n::t("ai_models.loading", language);
    }
    if let Some(error) = &state.error {
        if error == "catalog_needs_key" {
            return i18n::t("ai_models.needs_key", language);
        }
        if error == "catalog_needs_account" {
            return i18n::t("ai_models.needs_account", language);
        }
        return i18n::t("ai_models.unavailable", language);
    }
    i18n::t(
        if state.authoritative {
            "ai_models.live"
        } else {
            "ai_models.discovery"
        },
        language,
    )
}

/// Builds one option row for both hosts. The host-specific classes flow in so
/// the settings card and the agent menu keep their surface styling without a
/// second content model.
#[allow(clippy::too_many_arguments)]
pub fn model_option_row(
    item: &ModelItem,
    paint: &ModelRowPaint,
    actions: &ModelRowActions,
    row_class: &str,
) -> UiNode {
    let row_id = format!("{}option.{}", actions.node_prefix, item.value);
    let mut row = UiNode::new(row_id, UiNodeKind::Button)
        .with_class(if item.selected {
            format!("{row_class} {row_class}-selected")
        } else {
            row_class.to_string()
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(10.0, 8.0),
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_text_value(item.label.clone())
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(UiTextStyle::button(paint.text))
        .with_accessibility_role(UiAccessibilityRole::Option)
        .with_accessibility_selected(item.selected)
        .disabled(!item.available)
        .focusable();
    if !item.available {
        row = row.with_tooltip_key("ai_models.not_available");
    }
    if item.available && item.label != item.id {
        row = row.with_tooltip_value(item.id.clone());
    }
    row = row.with_event(UiEventBinding {
        event: UiEventKind::Click,
        action: UiAction::SetSelect {
            key: actions.select_key.clone(),
            value: item.value.clone(),
            index: 0,
        },
    });
    if let Some(close) = &actions.close_select_id {
        row = row.with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::SetSelectOpen {
                id: close.clone(),
                open: false,
            },
        });
    }
    row.with_child(pin_toggle(paint, actions, item))
}

fn pin_toggle(paint: &ModelRowPaint, actions: &ModelRowActions, item: &ModelItem) -> UiNode {
    let pin_command = format!("{}{}", actions.pin_command_prefix, item.value);
    let (label_key, color) = if item.pinned {
        ("ai_models.unpin", paint.accent_hot)
    } else {
        ("ai_models.pin", paint.muted)
    };
    UiNode::new(
        format!("{}pin.{}", actions.node_prefix, item.value),
        UiNodeKind::Button,
    )
    .with_class("model-pin-toggle")
    .with_layout(UiLayout::fixed(46.0, 24.0).padding_xy(4.0, 0.0))
    .with_text_key(label_key)
    .with_text_style(UiTextStyle::button(color))
    .with_accessibility_role(UiAccessibilityRole::Checkbox)
    .with_accessibility_checked(item.pinned)
    .focusable()
    .with_event(UiEventBinding::command(UiEventKind::Click, pin_command))
}

/// True when the pinned list still has room under the bounded local budget,
/// so runaway pinned lists can never bloat settings files forever.
pub fn pin_allowed(config: &raf_core::ai::AiProviderConfig) -> bool {
    config.pinned_models.len() < 32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_redesign_plan_does_not_mix_api_favorites_or_suggestions() {
        let mut config = raf_core::ai::AiProviderConfig::for_provider(AiProvider::OpenAI);
        config.model = "gpt-4o".into();
        config.pinned_models = vec!["gpt-4o".into(), "gpt-6-luna".into()];
        let account = raf_ai::chatgpt_auth::AuthView {
            accounts: vec![raf_ai::chatgpt_auth::AccountView {
                label: "Fixture".into(),
                active: true,
                connected: true,
            }],
            models: vec![raf_ai::chatgpt_auth::ModelView {
                slug: "gpt-6-luna".into(),
                label: "Luna".into(),
            }],
            ..Default::default()
        };
        let mut catalog = raf_ai::provider_catalog::Catalog::default();
        catalog.models = vec!["api-only".into()];
        catalog.fetched = true;
        let (_, items) = collect_from(&config, true, &account, &catalog, "");
        assert_eq!(
            items
                .iter()
                .filter(|m| m.available)
                .map(|m| m.value.as_str())
                .collect::<Vec<_>>(),
            vec!["account:gpt-6-luna"]
        );
        assert!(items
            .iter()
            .any(|m| m.selected && !m.available && m.id == "gpt-4o"));
        assert!(
            collect_from(&config, true, &Default::default(), &catalog, "")
                .1
                .iter()
                .all(|m| !m.available)
        );
    }
    #[test]
    fn provider_redesign_successful_empty_catalog_never_restores_curated_entries() {
        let config = raf_core::ai::AiProviderConfig::for_provider(AiProvider::OpenRouter);
        let mut catalog = raf_ai::provider_catalog::Catalog::default();
        catalog.fetched = true;
        assert!(
            collect_from(&config, false, &Default::default(), &catalog, "")
                .1
                .is_empty()
        );
        assert!(
            collect_from(&config, false, &Default::default(), &Default::default(), "")
                .1
                .len()
                > 0
        );
    }
    #[test]
    fn provider_redesign_filter_preserves_source_and_one_selected_model() {
        let mut config = raf_core::ai::AiProviderConfig::for_provider(AiProvider::OpenAI);
        config.model = "gpt-6.1-sol".into();
        let mut catalog = raf_ai::provider_catalog::Catalog::default();
        catalog.models = vec!["gpt-6.1-sol".into(), "gpt-6-luna".into()];
        catalog.fetched = true;
        let (status, items) = collect_from(&config, false, &Default::default(), &catalog, "SOL");
        assert!(status.authoritative && status.interleaved_curated == 0);
        assert_eq!(items.len(), 1);
        assert!(items[0].selected && items[0].available);
    }
}
