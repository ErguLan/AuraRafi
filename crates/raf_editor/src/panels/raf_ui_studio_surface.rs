//! Visible RafUI Studio workspace.
//!
//! This is intentionally a real retained surface instead of a Console dump.
//! The Console remains a command transport; Studio owns its own visual result.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiAction};
use raf_ui::{
    RafUiStudio, UiDocument, UiEnvironment, UiFlow, UiLayout, UiNode, UiNodeKind, UiSpacing,
    UiTextStyle,
};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

pub struct RafUiStudioSurfaceHost {
    bridge: RafUiSurfaceBridge,
}

impl Default for RafUiStudioSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_studio_workspace"),
        }
    }
}

impl RafUiStudioSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        document: &UiDocument,
        language: Language,
        scale_factor: f32,
    ) -> bool {
        let environment = UiEnvironment {
            viewport_size: [
                ui.available_width().max(1.0),
                ui.available_height().max(1.0),
            ],
            scale_factor,
            ..UiEnvironment::default()
        };
        let preview = RafUiStudio::new([1280, 720]).text_preview(document, environment);
        let surface = build_surface(palette, &preview);
        self.bridge
            .show(ui, render_state, palette, surface, |key| {
                let _ = language;
                key.to_string()
            })
            .into_iter()
            .any(|action| matches!(action.action, UiAction::Command { ref name } if name == "rafui-studio.close"))
    }
}

fn build_surface(palette: StudioUiPalette, preview: &raf_ui::UiStudioTextPreview) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("rafui-studio.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("rafui-studio.header", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: raf_ui::UiAlign::Center,
                    padding: UiSpacing::xy(22.0, 0.0),
                    ..UiLayout::fixed(0.0, 64.0)
                })
                .with_style(palette.toolbar_style())
                .with_child(
                    UiNode::new("rafui-studio.title", UiNodeKind::Label)
                        .with_text_key("RafUI Studio")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        }),
                )
                .with_child(
                    UiNode::new("rafui-studio.document", UiNodeKind::Label)
                        .with_text_key(format!(
                            "{}  |  {} nodes",
                            preview.document_name, preview.node_count
                        ))
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(260.0, 24.0)),
                )
                .with_child(
                    UiNode::new("rafui-studio.close", UiNodeKind::Button)
                        .with_text_key("Close Studio")
                        .with_text_style(UiTextStyle::button(tokens.text))
                        .with_layout(UiLayout::fixed(132.0, 34.0))
                        .focusable()
                        .with_event(raf_ui::UiEventBinding::command(
                            raf_ui::UiEventKind::Click,
                            "rafui-studio.close",
                        )),
                ),
        )
        .with_child(
            UiNode::new("rafui-studio.body", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    grow: 1.0,
                    gap: 1.0,
                    ..UiLayout::fill(UiFlow::Row)
                })
                .with_child(recipe_panel(palette, preview))
                .with_child(report_panel(palette, preview)),
        );
    UiSurface::new("rafui-studio", palette, root)
}

fn recipe_panel(palette: StudioUiPalette, preview: &raf_ui::UiStudioTextPreview) -> UiNode {
    let tokens = palette.tokens();
    let mut panel = UiNode::scroll_view("rafui-studio.recipes", raf_ui::UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            basis: [360.0, 0.0],
            min_size: [320.0, 0.0],
            padding: UiSpacing::same(22.0),
            gap: 10.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.panel_style())
        .with_child(title("Recipes", tokens.text))
        .with_child(label("Reusable RafUI components", tokens.text_muted));
    for recipe in &preview.recipes {
        panel = panel.with_child(
            UiNode::new(
                format!("rafui-studio.recipe.{}", recipe.name),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                padding: UiSpacing::xy(14.0, 6.0),
                gap: 2.0,
                ..UiLayout::fixed(0.0, 58.0)
            })
            .with_style(palette.subtle_panel_style())
            .with_child(
                UiNode::new(
                    format!("rafui-studio.recipe-name.{}", recipe.name),
                    UiNodeKind::Label,
                )
                .with_text_key(recipe.name.clone())
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 24.0)),
            )
            .with_child(
                UiNode::new(
                    format!("rafui-studio.recipe-meta.{}", recipe.name),
                    UiNodeKind::Label,
                )
                .with_text_key(format!(
                    "{}  |  min {}",
                    recipe.semantic_class,
                    recipe_min_size(recipe.min_size)
                ))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 20.0)),
            ),
        );
    }
    panel
}

fn report_panel(palette: StudioUiPalette, preview: &raf_ui::UiStudioTextPreview) -> UiNode {
    let tokens = palette.tokens();
    let density = &preview.selected_density.contract;
    let mut panel = UiNode::scroll_view("rafui-studio.report", raf_ui::UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            padding: UiSpacing::same(22.0),
            gap: 12.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.canvas_style())
        .with_child(title("Quality report", tokens.text))
        .with_child(label(
            format!(
                "Density  geometry {:.2}x  text {:.2}x  icons {:.2}x",
                density.geometry_scale, density.text_scale, density.icon_scale
            ),
            tokens.text,
        ))
        .with_child(label(
            format!(
                "Diagnostics  {}  |  {}",
                preview.diagnostic_count,
                if preview.diagnostics_clean {
                    "clean"
                } else {
                    "review required"
                }
            ),
            if preview.diagnostics_clean {
                tokens.positive
            } else {
                tokens.warning
            },
        ))
        .with_child(title("DPI matrix", tokens.text));
    for dpi in &preview.dpi_cases {
        panel = panel.with_child(label(
            format!(
                "{}  ->  {} x {}  |  geometry {:?}  text {:?}  icons {:?}",
                dpi.case.label(),
                dpi.physical_size[0],
                dpi.physical_size[1],
                dpi.density.geometry_snap,
                dpi.density.text_sampling,
                dpi.density.icon_sampling
            ),
            tokens.text_muted,
        ));
    }
    panel
}

fn title(text: impl Into<String>, color: [u8; 4]) -> UiNode {
    let text = text.into();
    UiNode::new(format!("rafui-studio.title.{text}"), UiNodeKind::Label)
        .with_text_key(text)
        .with_text_style(UiTextStyle::panel_title(color))
        .with_layout(UiLayout::fixed(0.0, 28.0))
}

fn label(text: impl Into<String>, color: [u8; 4]) -> UiNode {
    let text = text.into();
    UiNode::new(format!("rafui-studio.label.{}", text), UiNodeKind::Label)
        .with_text_key(text)
        .with_text_style(UiTextStyle::body(color))
        .with_layout(UiLayout::fixed(0.0, 24.0))
}

fn recipe_min_size(size: [f32; 2]) -> String {
    let width = if size[0] <= 0.0 {
        "fill".to_string()
    } else {
        format!("{:.0}", size[0])
    };
    format!("{} x {:.0}", width, size[1])
}
