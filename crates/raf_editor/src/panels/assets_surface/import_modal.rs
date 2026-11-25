//! Declarative import modal for the Assets panel.
//!
//! The surface never touches the filesystem. The host owns the file dialog,
//! the native drop event and the importer:
//!
//! - `assets.import-open` (toolbar) opens this modal and requests focus on
//!   [`ASSETS_IMPORT_NAME_ID`].
//! - `assets.import.browse` asks the host for a file.
//! - `assets.import.drop` is emitted on drag end over the drop zone; the host
//!   resolves the file from its native drop payload.
//! - `assets.import.cancel` closes the modal (Escape, backdrop and close).
//! - `assets.import.confirm` starts the import with the values below.
//! - `assets.import.name` (text), `assets.import.scale` (range),
//!   `assets.import.recenter`, `assets.import.collider` and
//!   `assets.import.textures` (toggles) are transient host state keys.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow,
    UiFontWeight, UiIcon, UiIconId, UiIconSize, UiJustify, UiLayout, UiNode, UiNodeKind,
    UiSizeMode, UiSpacing, UiStyle, UiSurface, UiSurfaceMaterial, UiTextOverflow, UiTextRole,
    UiTextStyle,
};
use raf_ui::{UiAction, UiRange, UiTextInput, UiToggle};

use crate::asset_previews::PreviewState;

use super::menus::overlay_surface;
use super::{ASSETS_IMPORT_MODAL_WIDTH, ASSETS_IMPORT_NAME_ID};

/// Coarse progress reported by the host while the importer runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AssetsImportPhase {
    Idle,
    Importing { progress: f32 },
    Completed,
    Failed,
}

impl Default for AssetsImportPhase {
    fn default() -> Self {
        Self::Idle
    }
}

/// Import options owned by the host. The modal only renders and emits keys.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetsImportOptions {
    pub name: String,
    pub scale: f32,
    pub recenter: bool,
    pub generate_collider: bool,
    pub import_textures: bool,
}

impl Default for AssetsImportOptions {
    fn default() -> Self {
        Self {
            name: String::new(),
            scale: 1.0,
            recenter: true,
            generate_collider: false,
            import_textures: true,
        }
    }
}

/// Everything the import modal needs to rebuild from data alone.
#[derive(Debug, Clone, Default)]
pub struct AssetsImportModalState<'a> {
    /// File name shown in the summary. `None` means no file was chosen.
    pub file_name: Option<&'a str>,
    /// Absolute or project-relative source path, when the host knows it.
    pub source_path: Option<&'a str>,
    pub file_size: Option<u64>,
    pub options: AssetsImportOptions,
    pub phase: AssetsImportPhase,
    /// Preview of the selected file. Models stay `Unavailable` until the
    /// backend provides one through `AssetPreviews::inject_model_preview`.
    pub preview: Option<PreviewState>,
    /// Importer error detail, already localized by the host.
    pub error: Option<&'a str>,
}

/// Dimmed backdrop that turns outside clicks into a modal dismissal.
pub fn build_assets_import_backdrop_surface(palette: StudioUiPalette) -> UiSurface {
    let root = UiNode::new("assets.import-backdrop", UiNodeKind::Overlay)
        .with_class("assets-backdrop")
        .with_material(UiSurfaceMaterial::BackdropScrim)
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_accessibility_label_key("electronics.footprint.import_file")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "assets.import.cancel",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.import.cancel",
        ));
    overlay_surface("editor.assets.import-backdrop", palette, root)
}

/// Builds the import dialog. The host centers it in window coordinates, the
/// same way it presents the other Assets modals.
pub fn build_assets_import_modal_surface(
    palette: StudioUiPalette,
    state: &AssetsImportModalState<'_>,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("assets.import-modal", UiNodeKind::FloatingPanel)
        .with_class("assets-modal assets-import-modal")
        .with_material(UiSurfaceMaterial::ModalSurface)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::xy(12.0, 10.0),
            ..UiLayout::fixed(ASSETS_IMPORT_MODAL_WIDTH, 0.0)
                .with_height_mode(UiSizeMode::FitContent)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        })
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_accessibility_label_key("electronics.footprint.import_file")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.import.cancel",
        ))
        .with_child(header(palette));

    root = root.with_child(drop_zone(palette));
    root = root.with_child(file_summary(palette, state));
    root = root.with_child(options(palette, &state.options));
    if let Some(status) = progress_or_error(palette, state) {
        root = root.with_child(status);
    }
    root = root.with_child(footer(palette, state));

    overlay_surface("editor.assets.import-modal", palette, root)
}

fn header(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("assets.import-modal.header", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("assets.import-modal.title", UiNodeKind::Label)
                .with_text_key("electronics.footprint.import_file")
                .with_text_style(UiTextStyle {
                    role: UiTextRole::PanelTitle,
                    size_px: 13.0,
                    line_height_px: 18.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)
                }),
        )
        .with_child(
            UiNode::new("assets.import-modal.close", UiNodeKind::Button)
                .with_class("asset-action")
                .with_layout(UiLayout::fixed(22.0, 22.0))
                .with_icon(
                    UiIcon::new(UiIconId::Close)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.text_muted),
                )
                .with_tooltip_key("app.close")
                .with_accessibility_label_key("app.close")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "assets.import.cancel",
                )),
        )
}

fn drop_zone(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("assets.import-modal.dropzone", UiNodeKind::Panel)
        .with_class("assets-import-dropzone")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 6.0,
            padding: UiSpacing::xy(12.0, 10.0),
            ..UiLayout::fixed(0.0, 120.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 5.0,
            opacity: 1.0,
        })
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key("app.drag_drop_hint")
        .with_tooltip_key("app.drag_drop_hint")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "assets.import.browse",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            "assets.import.drop",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            "assets.import.browse",
        ))
        .with_child(
            UiNode::new("assets.import-modal.dropzone.icon", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(UiIconId::File)
                        .with_size(UiIconSize::Custom(28))
                        .with_tint(tokens.accent),
                )
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("assets.import-modal.dropzone.hint", UiNodeKind::Label)
                .with_text_key("app.drag_drop_hint")
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("assets.import-modal.dropzone.browse", UiNodeKind::Button)
                .with_class("asset-action")
                .with_text_key("app.open")
                .with_layout(UiLayout::fixed(86.0, 24.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "assets.import.browse",
                )),
        )
}

fn file_summary(palette: StudioUiPalette, state: &AssetsImportModalState<'_>) -> UiNode {
    let tokens = palette.tokens();
    let name = state.file_name.unwrap_or("—");
    let kind = state
        .file_name
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_uppercase())
        .unwrap_or_else(|| "—".to_string());
    let size = state
        .file_size
        .map(format_bytes)
        .unwrap_or_else(|| "—".to_string());

    let mut summary = UiNode::new("assets.import-modal.file", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 12.0,
            padding: UiSpacing::xy(8.0, 6.0),
            ..UiLayout::fixed(0.0, 96.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(preview_tile(palette, state.preview.as_ref()));
    summary = summary.with_child(
        UiNode::new("assets.import-modal.file.text", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 3.0,
                ..UiLayout::fixed(0.0, 0.0)
                    .with_width_mode(UiSizeMode::Fill)
                    .with_height_mode(UiSizeMode::FitContent)
            })
            .with_child(
                UiNode::new("assets.import-modal.file.name", UiNodeKind::Label)
                    .with_text_value(name)
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::PanelTitle,
                        size_px: 13.0,
                        line_height_px: 18.0,
                        weight: UiFontWeight::Bold,
                        color: tokens.text,
                        inherit_color: false,
                    })
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)),
            )
            .with_child(
                UiNode::new("assets.import-modal.file.meta", UiNodeKind::Label)
                    .with_text_value(format!("{size} · {kind}"))
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Monospace,
                        size_px: 10.0,
                        line_height_px: 14.0,
                        weight: UiFontWeight::Regular,
                        color: tokens.text_muted,
                        inherit_color: false,
                    })
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_layout(UiLayout::fixed(0.0, 14.0).with_width_mode(UiSizeMode::Fill)),
            )
            .with_child(
                UiNode::new("assets.import-modal.file.path", UiNodeKind::Label)
                    .with_text_value(state.source_path.unwrap_or("—"))
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Monospace,
                        size_px: 10.0,
                        line_height_px: 13.0,
                        weight: UiFontWeight::Regular,
                        color: tokens.text_muted,
                        inherit_color: false,
                    })
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_layout(UiLayout::fixed(0.0, 13.0).with_width_mode(UiSizeMode::Fill)),
            ),
    );
    summary
}

fn preview_tile(palette: StudioUiPalette, preview: Option<&PreviewState>) -> UiNode {
    let tokens = palette.tokens();
    let mut tile = UiNode::new("assets.import-modal.preview", UiNodeKind::Panel)
        .with_class("assets-import-preview")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 4.0,
            ..UiLayout::fixed(84.0, 84.0)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 4.0,
            opacity: 1.0,
        });
    match preview {
        Some(PreviewState::Ready { key, .. }) => {
            let mut image = UiNode::new("assets.import-modal.preview.image", UiNodeKind::Label)
                .with_layout(UiLayout::fixed(78.0, 78.0));
            raf_ui::components::set_thumbnail_image(
                &mut image,
                key.clone(),
                raf_ui::UiImageFit::Contain,
                4.0,
            );
            tile = tile.with_child(image);
        }
        Some(PreviewState::Loading) => {
            tile = tile.with_child(preview_placeholder(
                UiIconId::Refresh,
                "assets.preview_loading",
                tokens.info,
            ));
        }
        Some(PreviewState::Missing) => {
            tile = tile.with_child(preview_placeholder(
                UiIconId::Warning,
                "assets.missing_file",
                tokens.danger,
            ));
        }
        Some(PreviewState::Error(_)) => {
            tile = tile.with_child(preview_placeholder(
                UiIconId::Warning,
                "assets.preview_error",
                tokens.danger,
            ));
        }
        Some(PreviewState::Unavailable(key)) => {
            tile = tile.with_child(preview_placeholder(UiIconId::Warning, key, tokens.warning));
        }
        None => {
            tile = tile.with_child(preview_placeholder(
                UiIconId::File,
                "assets.preview_unavailable",
                tokens.text_muted,
            ));
        }
    }
    tile
}

fn preview_placeholder(icon: UiIconId, label_key: &str, tint: [u8; 4]) -> UiNode {
    UiNode::new("assets.import-modal.preview.placeholder", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            gap: 3.0,
            ..UiLayout::fit_content()
        })
        .with_child(
            UiNode::new("assets.import-modal.preview.icon", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(icon)
                        .with_size(UiIconSize::Panel)
                        .with_tint(tint),
                )
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("assets.import-modal.preview.label", UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 9.0,
                    line_height_px: 11.0,
                    weight: UiFontWeight::Regular,
                    color: tint,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(76.0, 22.0)),
        )
}

fn options(palette: StudioUiPalette, options: &AssetsImportOptions) -> UiNode {
    let tokens = palette.tokens();
    let mut column =
        UiNode::new("assets.import-modal.options", UiNodeKind::Panel).with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 0.0)
                .with_width_mode(UiSizeMode::Fill)
                .with_height_mode(UiSizeMode::FitContent)
        });

    column = column.with_child(
        UiNode::text_input(
            ASSETS_IMPORT_NAME_ID,
            UiTextInput {
                value_key: ASSETS_IMPORT_NAME_ID.to_string(),
                placeholder_key: Some("app.name".to_string()),
                max_length: 120,
                multiline: false,
                password: false,
                submit_command: Some("assets.import.confirm".to_string()),
            },
        )
        .with_class("asset-script-name")
        .with_accessibility_label_key("app.name")
        .with_layout(UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.import.cancel",
        )),
    );

    column = column.with_child(
        UiNode::new("assets.import-modal.options.scale-row", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new("assets.import-modal.options.scale-label", UiNodeKind::Label)
                    .with_text_key("app.scale")
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fixed(84.0, 16.0)),
            )
            .with_child(
                UiNode::range(
                    "assets.import.scale",
                    UiRange::new("assets.import.scale", options.scale, 0.01, 100.0, 0.01),
                )
                .with_class("inspector-range")
                .with_layout(UiLayout {
                    grow: 1.0,
                    min_size: [120.0, 24.0],
                    ..UiLayout::fixed(0.0, 24.0)
                }),
            )
            .with_child(
                UiNode::new("assets.import-modal.options.scale-value", UiNodeKind::Label)
                    .with_text_value(format!("{:.2}×", options.scale))
                    .with_text_style(UiTextStyle::button(tokens.text_muted))
                    .with_layout(UiLayout::fixed(56.0, 24.0).with_text_safe_area(true)),
            ),
    );

    column = column.with_child(
        UiNode::new("assets.import-modal.options.toggles", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 2.0,
                ..UiLayout::fixed(0.0, 0.0)
                    .with_width_mode(UiSizeMode::Fill)
                    .with_height_mode(UiSizeMode::FitContent)
            })
            .with_child(toggle_row(
                palette,
                "assets.import-modal.options.recenter",
                "app.reset_transform",
                "assets.import.recenter",
                options.recenter,
            ))
            .with_child(toggle_row(
                palette,
                "assets.import-modal.options.collider",
                "app.collider",
                "assets.import.collider",
                options.generate_collider,
            ))
            .with_child(toggle_row(
                palette,
                "assets.import-modal.options.textures",
                "app.base_color_texture",
                "assets.import.textures",
                options.import_textures,
            )),
    );
    column
}

fn toggle_row(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    value_key: &str,
    value: bool,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)
                }),
        )
        .with_child(
            UiNode::toggle(value_key, UiToggle::new(value_key, value))
                .with_class("inspector-toggle")
                .with_layout(UiLayout::fixed(36.0, 24.0))
                .focusable()
                .with_event(UiEventBinding {
                    event: UiEventKind::Click,
                    action: UiAction::SetToggle {
                        key: value_key.to_string(),
                        value: !value,
                    },
                })
                // The host already routes these names to its option state.
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    value_key.to_string(),
                )),
        )
}

fn progress_or_error(
    palette: StudioUiPalette,
    state: &AssetsImportModalState<'_>,
) -> Option<UiNode> {
    let tokens = palette.tokens();
    let progress = match state.phase {
        AssetsImportPhase::Importing { progress } => {
            let progress = if progress.is_finite() {
                progress.clamp(0.0, 1.0)
            } else {
                0.0
            };
            Some(
                UiNode::new("assets.import-modal.progress", UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        flow: UiFlow::Column,
                        gap: 3.0,
                        ..UiLayout::fixed(0.0, 0.0)
                            .with_width_mode(UiSizeMode::Fill)
                            .with_height_mode(UiSizeMode::FitContent)
                    })
                    .with_child(
                        UiNode::new("assets.import-modal.progress.label", UiNodeKind::Label)
                            .with_text_key("app.assets.import_requested")
                            .with_text_style(UiTextStyle::body(tokens.info))
                            .with_layout(UiLayout::fit_content()),
                    )
                    .with_child(
                        UiNode::range(
                            "assets.import.progress",
                            UiRange::new(
                                "assets.import.progress",
                                progress * 100.0,
                                0.0,
                                100.0,
                                1.0,
                            ),
                        )
                        .with_layout(UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill))
                        .disabled(true),
                    )
                    .with_child(
                        UiNode::new("assets.import-modal.progress.value", UiNodeKind::Label)
                            .with_text_value(format!("{:.0}%", progress * 100.0))
                            .with_text_style(UiTextStyle::body(tokens.text_muted))
                            .with_layout(UiLayout::fit_content()),
                    ),
            )
        }
        _ => None,
    };
    let error = match state.error {
        Some(detail) => Some(status_line(
            palette,
            "assets.import-modal.error",
            UiIconId::Error,
            "app.error",
            tokens.danger,
            Some(detail),
        )),
        None if state.phase == AssetsImportPhase::Failed => Some(status_line(
            palette,
            "assets.import-modal.error",
            UiIconId::Error,
            "app.error",
            tokens.danger,
            None,
        )),
        None => None,
    };
    match (progress, error) {
        (Some(progress), Some(error)) => Some(
            UiNode::new("assets.import-modal.status", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 3.0,
                    ..UiLayout::fixed(0.0, 0.0)
                        .with_width_mode(UiSizeMode::Fill)
                        .with_height_mode(UiSizeMode::FitContent)
                })
                .with_child(progress)
                .with_child(error),
        ),
        (Some(progress), None) => Some(progress),
        (None, Some(error)) => Some(error),
        (None, None) if state.phase == AssetsImportPhase::Completed => Some(status_line(
            palette,
            "assets.import-modal.completed",
            UiIconId::Success,
            "app.assets.imported",
            tokens.positive,
            None,
        )),
        (None, None) => None,
    }
}

fn status_line(
    palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    label_key: &str,
    tint: [u8; 4],
    detail: Option<&str>,
) -> UiNode {
    let mut line = UiNode::new(id, UiNodeKind::Panel)
        .with_class("assets-status-line")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(8.0, 3.0),
            ..UiLayout::fixed(0.0, 0.0)
                .with_width_mode(UiSizeMode::Fill)
                .with_height_mode(UiSizeMode::FitContent)
        })
        .with_icon(
            UiIcon::new(icon)
                .with_size(UiIconSize::Small)
                .with_tint(tint),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tint))
                .with_layout(UiLayout::fit_content()),
        );
    if let Some(detail) = detail {
        line = line.with_child(
            UiNode::new(format!("{id}.detail"), UiNodeKind::Label)
                .with_text_value(detail)
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 14.0).with_width_mode(UiSizeMode::Fill)
                }),
        );
    }
    line
}

fn footer(palette: StudioUiPalette, state: &AssetsImportModalState<'_>) -> UiNode {
    let tokens = palette.tokens();
    let importing = matches!(state.phase, AssetsImportPhase::Importing { .. });
    UiNode::new("assets.import-modal.footer", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(0.0, 4.0),
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("assets.import-modal.footer.spacer", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(0.0, 1.0).with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("assets.import-modal.footer.cancel", UiNodeKind::Button)
                .with_class("asset-script-cancel")
                .with_text_key("app.cancel")
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_layout(UiLayout::fixed(86.0, 26.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "assets.import.cancel",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::KeyPress("escape".to_string()),
                    "assets.import.cancel",
                )),
        )
        .with_child(
            UiNode::new("assets.import-modal.footer.confirm", UiNodeKind::Button)
                .with_class("assets-modal-confirm")
                .with_text_key("electronics.footprint.import_file")
                .with_layout(UiLayout::fixed(104.0, 26.0))
                .disabled(state.file_name.is_none() || importing)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "assets.import.confirm",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::KeyPress("enter".to_string()),
                    "assets.import.confirm",
                )),
        )
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{} B", bytes as u64)
    }
}
