//! Lightweight retained UI surface for ApiGraphicBasic.
//!
//! This is the retained UI contract for editor chrome and canvas
//! overlays. It stores layout and text keys as data, compiles one retained
//! paint list for CPU or GPU composition, and does not depend on the legacy
//! editor shell.

mod application_menu;
mod compilation;
mod cpu_host;
mod cpu_renderer;
mod diagnostics;
mod direct_host;
mod gpu_renderer;
mod images;
mod native_input;
mod native_window;
mod presentation;
mod quality;
mod render;
mod text_atlas;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use application_menu::{NativeApplicationMenuAdapter, NativeWindowApplicationMenuAdapter};
pub use compilation::UiSurfaceCompileMetrics;
pub use cpu_host::{CpuUiSurfaceHost, DirectUiSurfaceCpuFrame};
pub use cpu_renderer::{UiSurfaceCpuMetrics, UiSurfaceCpuRenderer};
pub use diagnostics::UiSurfaceDiagnostics;
pub use direct_host::{DirectUiSurfaceFrame, DirectUiSurfaceHost};
pub use gpu_renderer::{UiSurfaceGpuMetrics, UiSurfaceGpuRenderer};
pub use images::{UiSurfaceImageData, UiSurfaceImageStore};
pub use native_input::NativeUiInputBridge;
pub use native_window::{
    NativeGraphicsContext, NativeUiWindowConfig, NativeUiWindowHost, NativeWindowCommandResult,
};
pub use presentation::{
    UiSurfaceDrawList, UiSurfaceImageQuad, UiSurfacePaintCommand, UiSurfaceQuad, UiSurfaceTextQuad,
};
pub use quality::{cpu_quality_matrix, UiSurfaceQualitySample};
pub use raf_ui::{
    BottomDockLayout, DockDropTarget, DockLayout, DockLayoutEntry, DockLayoutFrame, DockPanel,
    DockSide, DockTab, DockTabGroup, DockWorkspaceController, DockWorkspaceEvent, FloatingPanel,
    StudioUiPalette, UiAction, UiAlign, UiColorMode, UiCompactMode, UiControl, UiControlState,
    UiCursorIcon, UiDensityContract, UiDispatchedAction, UiEnvironment, UiEventBinding,
    UiEventKind, UiFlow, UiFocusPolicy, UiFocusState, UiFontWeight, UiGeometrySnap, UiGridLayout,
    UiHitRegion, UiHitResult, UiHitTestMode, UiIcon, UiIconId, UiIconSize, UiImage, UiImageFit,
    UiImageSource, UiInputState, UiInteractionState, UiJustify, UiLayout, UiNode, UiNodeKind,
    UiOverflow, UiPointerButton, UiPositionMode, UiRange, UiRect, UiResponsiveRule, UiSamplingMode,
    UiScrollAxis, UiSizeMode, UiSkeleton, UiSkeletonShape, UiSpacing, UiStyle, UiStylePatch,
    UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextAtlasRequest,
    UiTextEditState, UiTextInput, UiTextRole, UiTextStyle, UiTheme, UiThemeMetrics, UiToggle,
    UiTokens, UiTween, UiVirtualRange, UiVisualState, BOTTOM_DOCK_LAYOUT_VERSION,
    FLOATING_PANEL_RESIZE_HANDLE_SIZE, FLOATING_PANEL_TITLE_BAR_HEIGHT,
};
pub use render::{UiLayoutBox, UiScrollMetrics, UiSurfaceFrame};
pub use text_atlas::{UiTextAtlas, UiTextAtlasRect, UiTextAtlasSlot, UiTextAtlasSyncStats};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSurface {
    pub id: String,
    pub palette: StudioUiPalette,
    #[serde(default)]
    pub theme: UiTheme,
    pub root: UiNode,
    pub style_sheet: UiStyleSheet,
    #[serde(default = "default_retained_tooltips")]
    pub retained_tooltips: bool,
}

fn default_retained_tooltips() -> bool {
    true
}

impl UiSurface {
    pub fn new(id: impl Into<String>, palette: StudioUiPalette, root: UiNode) -> Self {
        Self {
            id: id.into(),
            palette,
            theme: UiTheme::raf_ui(),
            root,
            style_sheet: UiStyleSheet::default(),
            retained_tooltips: true,
        }
    }

    pub fn from_document(document: &raf_ui::UiDocument, palette: StudioUiPalette) -> Self {
        Self::new(document.id.0.to_string(), palette, document.root.clone())
            .with_theme(document.theme.clone())
    }

    pub fn with_theme(mut self, theme: UiTheme) -> Self {
        self.theme = theme;
        self
    }

    pub fn with_retained_tooltips(mut self, enabled: bool) -> Self {
        self.retained_tooltips = enabled;
        self
    }

    pub fn theme_tokens(&self, environment: UiEnvironment) -> UiTokens {
        let system_dark = matches!(self.palette, StudioUiPalette::IndustrialDark);
        self.theme.tokens_for(environment.color_mode, system_dark)
    }

    pub fn build_frame(&self, width: u32, height: u32, clear_color: [u8; 4]) -> UiSurfaceFrame {
        self.build_frame_with_visual_state(width, height, clear_color, UiVisualState::default())
    }

    pub fn build_frame_with_visual_state(
        &self,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
        visual_state: UiVisualState<'_>,
    ) -> UiSurfaceFrame {
        self.build_frame_with_state(
            width,
            height,
            clear_color,
            visual_state,
            &UiControlState::default(),
        )
    }

    pub fn build_frame_with_state(
        &self,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
        visual_state: UiVisualState<'_>,
        control_state: &UiControlState,
    ) -> UiSurfaceFrame {
        self.build_frame_with_state_and_tooltip_alpha(
            width,
            height,
            clear_color,
            visual_state,
            control_state,
            1.0,
        )
    }

    pub fn build_frame_with_state_and_tooltip_alpha(
        &self,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
        visual_state: UiVisualState<'_>,
        control_state: &UiControlState,
        tooltip_alpha: f32,
    ) -> UiSurfaceFrame {
        self.build_frame_with_state_and_tooltip_alpha_and_pointer(
            width,
            height,
            clear_color,
            visual_state,
            control_state,
            tooltip_alpha,
            None,
        )
    }

    pub fn build_frame_with_state_and_tooltip_alpha_and_pointer(
        &self,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
        visual_state: UiVisualState<'_>,
        control_state: &UiControlState,
        tooltip_alpha: f32,
        pointer_position: Option<[f32; 2]>,
    ) -> UiSurfaceFrame {
        let mut frame = render::build_surface_frame(
            &self.root,
            &self.style_sheet,
            visual_state,
            control_state,
            width,
            height,
            clear_color,
        );
        if self.retained_tooltips {
            append_hover_tooltip(
                &mut frame,
                visual_state,
                self.palette,
                tooltip_alpha,
                pointer_position,
            );
        }
        frame
    }

    pub(crate) fn build_frame_with_intrinsic_sizes(
        &self,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
        visual_state: UiVisualState<'_>,
        control_state: &UiControlState,
        tooltip_alpha: f32,
        pointer_position: Option<[f32; 2]>,
        intrinsic_sizes: &HashMap<String, [f32; 2]>,
    ) -> UiSurfaceFrame {
        let mut frame = render::build_surface_frame_with_intrinsic_sizes(
            &self.root,
            &self.style_sheet,
            visual_state,
            control_state,
            width,
            height,
            clear_color,
            intrinsic_sizes,
        );
        if self.retained_tooltips {
            append_hover_tooltip(
                &mut frame,
                visual_state,
                self.palette,
                tooltip_alpha,
                pointer_position,
            );
        }
        frame
    }

    pub fn hit_test<'a>(
        &'a self,
        layout_boxes: &'a [UiLayoutBox],
        point: [f32; 2],
    ) -> Option<&'a UiLayoutBox> {
        layout_boxes
            .iter()
            .enumerate()
            .filter(|(_, layout_box)| {
                layout_box.interactive
                    && !layout_box.disabled
                    && layout_box.rect.contains(point)
                    && layout_box.clip_rect.contains(point)
            })
            .max_by_key(|(sequence, layout_box)| (layout_box.z_index, *sequence))
            .map(|(_, layout_box)| layout_box)
    }

    pub fn hit_test_regions(
        &self,
        hit_regions: &[UiHitRegion],
        point: [f32; 2],
    ) -> Option<UiHitResult> {
        raf_ui::hit_test(hit_regions, point, UiHitTestMode::InteractiveOnly)
    }
}

fn tooltip_key_for_node(node: &UiNode, id: &str) -> Option<String> {
    if node.id == id {
        return node.tooltip_key.clone();
    }
    node.children
        .iter()
        .find_map(|child| tooltip_key_for_node(child, id))
}

fn tooltip_value_for_node(node: &UiNode, id: &str) -> Option<String> {
    if node.id == id {
        return node.tooltip_value.clone();
    }
    node.children
        .iter()
        .find_map(|child| tooltip_value_for_node(child, id))
}

fn append_hover_tooltip(
    frame: &mut UiSurfaceFrame,
    visual_state: UiVisualState<'_>,
    palette: StudioUiPalette,
    tooltip_alpha: f32,
    pointer_position: Option<[f32; 2]>,
) {
    if tooltip_alpha <= 0.001 {
        return;
    }
    let target_id = visual_state
        .hovered_id
        .filter(|id| !id.starts_with("__rafui.tooltip"));
    let Some(target_id) = target_id else { return };
    let Some(target) = frame
        .layout_boxes
        .iter()
        .find(|entry| {
            entry.id == target_id
                && entry.interactive
                && !entry.disabled
                && (entry.tooltip_key.is_some() || entry.tooltip_value.is_some())
        })
        .cloned()
    else {
        return;
    };
    let tooltip_key = target.tooltip_key.clone();
    let tooltip_value = target.tooltip_value.clone();
    if tooltip_key.is_none() && tooltip_value.is_none() {
        return;
    }
    let Some(bounds) = frame
        .layout_boxes
        .iter()
        .find(|entry| entry.kind == UiNodeKind::Root)
        .map(|entry| entry.rect)
    else {
        return;
    };

    let tooltip_size = [96.0_f32.min(bounds.width.max(1.0)), 24.0_f32];
    let gap = 8.0;
    let _ = pointer_position;
    let anchor = pointer_position
        .map(|point| UiRect::new(point[0], point[1], 1.0, 1.0))
        .unwrap_or(target.rect);
    frame.tooltip_anchor = Some(anchor);
    let anchor_x = anchor.x;
    let below_y = anchor.bottom() + gap;
    let above_y = anchor.y - tooltip_size[1] - gap;
    let y = if below_y + tooltip_size[1] <= bounds.bottom() - 4.0 {
        below_y
    } else {
        above_y
    };
    let rect = UiRect::new(anchor_x, y, tooltip_size[0], tooltip_size[1])
        .clamp_inside(bounds.shrink(UiSpacing::same(4.0)));
    let tooltip_alpha = tooltip_alpha.clamp(0.0, 1.0);
    let text_color = [232, 234, 238, 255];
    let tooltip_id = "__rafui.tooltip".to_string();
    frame.layout_boxes.push(UiLayoutBox {
        id: tooltip_id.clone(),
        kind: UiNodeKind::Tooltip,
        text_key: tooltip_key.clone(),
        text_value: tooltip_value.clone(),
        tooltip_key: None,
        tooltip_value: None,
        rect,
        content_rect: rect,
        clip_rect: bounds,
        interactive: false,
        focusable: false,
        disabled: false,
        accessibility_label_key: None,
        z_index: i16::MAX,
        width_mode: UiSizeMode::FitContent,
        height_mode: UiSizeMode::FitContent,
        style: UiStyle {
            fill: if matches!(palette, StudioUiPalette::IndustrialDark) {
                [58, 61, 65, 232]
            } else {
                [70, 73, 78, 224]
            },
            border: if matches!(palette, StudioUiPalette::IndustrialDark) {
                [104, 108, 115, 220]
            } else {
                [136, 140, 148, 216]
            },
            text: [232, 234, 238, 255],
            border_width: 1.0,
            radius: 4.0,
            opacity: 0.94 * tooltip_alpha,
        },
        text_style: Some(UiTextStyle {
            role: UiTextRole::Tooltip,
            size_px: 10.5,
            line_height_px: 14.0,
            weight: UiFontWeight::Regular,
            color: text_color,
            inherit_color: false,
        }),
        icon: None,
        control: UiControl::None,
        text_edit: None,
    });
    frame.text_requests.push(UiTextAtlasRequest::new(
        tooltip_id,
        tooltip_key
            .or(tooltip_value)
            .expect("tooltip has a key or literal value"),
        UiTextStyle {
            role: UiTextRole::Tooltip,
            size_px: 10.5,
            line_height_px: 14.0,
            weight: UiFontWeight::Regular,
            color: text_color,
            inherit_color: false,
        },
        (bounds.width - 8.0).max(1.0),
    ));
}

/// Stateful companion for a retained `UiSurface`.
///
/// The surface itself stays serializable and declarative. This session owns
/// only frame-local interaction and text-cache state, so it can be recreated
/// when an editor workspace changes without invalidating persisted layouts.
#[derive(Debug, Clone, Default)]
pub struct UiSurfaceSession {
    pub text_atlas: UiTextAtlas,
    pub interaction: UiInteractionState,
    pub focus_policy: UiFocusPolicy,
    tooltip_motion: UiTween,
    reduced_motion: bool,
    last_input_time_seconds: f64,
    tooltip_input_seen: bool,
    last_raster_scale: f32,
}

impl UiSurfaceSession {
    pub fn transient_motion_revision(&self) -> u32 {
        self.tooltip_motion.value().to_bits()
    }

    /// True while RafUI still has a tooltip transition that needs another
    /// retained frame. Native hosts use this to keep event-driven rendering
    /// alive until the interaction settles.
    pub fn has_active_motion(&self) -> bool {
        !self.tooltip_motion.is_settled()
    }

    /// Lets a native host honor `UiEnvironment::prefers_reduced_motion`
    /// without coupling the retained surface to a windowing API.
    pub fn set_reduced_motion(&mut self, reduced_motion: bool) {
        self.reduced_motion = reduced_motion;
    }

    /// Resets transient interaction when a host swaps the document's logical
    /// content, such as changing Agent sessions or message pages.
    pub fn reset_interaction_for_surface_change(&mut self, scroll_id: Option<&str>) {
        self.interaction.reset_for_surface_change();
        if let Some(scroll_id) = scroll_id {
            self.interaction.controls.reset_scroll(scroll_id);
        }
        self.tooltip_motion.set_immediate(0.0);
        self.last_input_time_seconds = 0.0;
        self.tooltip_input_seen = false;
    }

    pub fn hovered_tooltip_key(&self, surface: &UiSurface) -> Option<String> {
        let hovered = self.interaction.focus.hovered.as_deref()?;
        tooltip_key_for_node(&surface.root, hovered)
    }

    pub fn hovered_tooltip_value(&self, surface: &UiSurface) -> Option<String> {
        let hovered = self.interaction.focus.hovered.as_deref()?;
        tooltip_value_for_node(&surface.root, hovered)
    }

    pub fn cursor_hint(&self, surface: &UiSurface) -> raf_ui::UiCursorIcon {
        self.interaction.cursor_hint(&surface.root)
    }

    pub fn captures_keyboard_input(&self, surface: &UiSurface) -> bool {
        self.interaction.captures_keyboard_input(&surface.root)
    }

    pub fn build_frame(
        &mut self,
        surface: &UiSurface,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
    ) -> UiSurfaceFrame {
        let frame = self.build_layout_frame_at_scale(surface, width, height, clear_color, 1.0);
        self.text_atlas.sync(&frame.text_requests);
        frame
    }

    /// Builds only the retained layout and input data. Presentation hosts use
    /// this as their cacheable stage before resolving text and compiling paint
    /// geometry for a CPU or GPU target.
    pub fn build_layout_frame_at_scale(
        &mut self,
        surface: &UiSurface,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
        raster_scale: f32,
    ) -> UiSurfaceFrame {
        let raster_scale = raster_scale.clamp(1.0, 4.0);
        self.last_raster_scale = raster_scale;
        let tooltip_alpha = if !self.tooltip_input_seen && self.interaction.focus.hovered.is_some()
        {
            // Deterministic/headless callers may set a hover target directly
            // without sending a native input snapshot. Preserve that useful
            // API while real pointer input still uses the hover-intent delay.
            1.0
        } else {
            self.tooltip_motion.value()
        };
        let mut frame = surface.build_frame_with_state_and_tooltip_alpha_and_pointer(
            width,
            height,
            clear_color,
            UiVisualState::from_focus(&self.interaction.focus),
            &self.interaction.controls,
            tooltip_alpha,
            self.interaction.pointer_position(),
        );
        if raster_scale > 1.0 {
            frame.text_requests = frame
                .text_requests
                .iter()
                .map(|request| request.scaled_for_raster(raster_scale))
                .collect();
        }
        let has_intrinsic_sizing = frame.layout_boxes.iter().any(|layout| {
            matches!(
                layout.width_mode,
                UiSizeMode::FitContent | UiSizeMode::MinContent | UiSizeMode::MaxContent
            ) || matches!(
                layout.height_mode,
                UiSizeMode::FitContent | UiSizeMode::MinContent | UiSizeMode::MaxContent
            )
        });
        for metrics in &frame.scroll_metrics {
            if has_intrinsic_sizing {
                self.interaction
                    .controls
                    .set_scroll_metrics_preserving_offset(metrics.id.clone(), metrics.max_offset);
            } else {
                self.interaction
                    .controls
                    .set_scroll_metrics(metrics.id.clone(), metrics.max_offset);
            }
        }
        self.reconcile_focus(&frame);
        frame
    }

    pub fn build_frame_with_resolved_text<F>(
        &mut self,
        surface: &UiSurface,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
        resolve: F,
    ) -> UiSurfaceFrame
    where
        F: FnMut(&str) -> String,
    {
        self.build_frame_with_resolved_text_at_scale(
            surface,
            width,
            height,
            clear_color,
            1.0,
            resolve,
        )
    }

    /// Builds logical layout coordinates while rasterizing text at the output
    /// density requested by a presentation host. The layout remains in points
    /// so pointer input and document metrics do not change with monitor DPI.
    pub fn build_frame_with_resolved_text_at_scale<F>(
        &mut self,
        surface: &UiSurface,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
        raster_scale: f32,
        mut resolve: F,
    ) -> UiSurfaceFrame
    where
        F: FnMut(&str) -> String,
    {
        let frame =
            self.build_layout_frame_at_scale(surface, width, height, clear_color, raster_scale);
        let resolved_text = self.resolve_text_requests(&frame, |key| resolve(key));
        self.sync_resolved_text(&frame, &resolved_text);
        let intrinsic_sizes = self.intrinsic_sizes_for_frame(&frame, &resolved_text, raster_scale);
        if intrinsic_sizes.is_empty() {
            return frame;
        }
        self.rebuild_layout_with_intrinsic_sizes(
            surface,
            width,
            height,
            clear_color,
            raster_scale,
            &intrinsic_sizes,
        )
    }

    pub(super) fn intrinsic_sizes_for_frame(
        &self,
        frame: &UiSurfaceFrame,
        resolved_text: &[String],
        raster_scale: f32,
    ) -> HashMap<String, [f32; 2]> {
        let raster_scale = raster_scale.clamp(1.0, 4.0);
        frame
            .text_requests
            .iter()
            .zip(resolved_text.iter())
            .filter_map(|(request, resolved)| {
                let layout = frame
                    .layout_boxes
                    .iter()
                    .find(|entry| entry.id == request.node_id)?;
                if !matches!(
                    (layout.width_mode, layout.height_mode),
                    (UiSizeMode::FitContent, _)
                        | (UiSizeMode::MinContent, _)
                        | (UiSizeMode::MaxContent, _)
                        | (_, UiSizeMode::FitContent)
                        | (_, UiSizeMode::MinContent)
                        | (_, UiSizeMode::MaxContent)
                ) {
                    return None;
                }
                let slot = self.text_atlas.slot_for(request, resolved.as_str())?;
                Some((
                    request.node_id.clone(),
                    [
                        f32::from(slot.rect.width) / raster_scale
                            + (layout.rect.width - layout.content_rect.width).max(0.0),
                        f32::from(slot.rect.height) / raster_scale
                            + (layout.rect.height - layout.content_rect.height).max(0.0),
                    ],
                ))
            })
            .collect()
    }

    pub(super) fn rebuild_layout_with_intrinsic_sizes(
        &mut self,
        surface: &UiSurface,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
        raster_scale: f32,
        intrinsic_sizes: &HashMap<String, [f32; 2]>,
    ) -> UiSurfaceFrame {
        let tooltip_alpha = if !self.tooltip_input_seen && self.interaction.focus.hovered.is_some()
        {
            1.0
        } else {
            self.tooltip_motion.value()
        };
        let mut frame = surface.build_frame_with_intrinsic_sizes(
            width,
            height,
            clear_color,
            UiVisualState::from_focus(&self.interaction.focus),
            &self.interaction.controls,
            tooltip_alpha,
            self.interaction.pointer_position(),
            intrinsic_sizes,
        );
        let raster_scale = raster_scale.clamp(1.0, 4.0);
        if raster_scale > 1.0 {
            frame.text_requests = frame
                .text_requests
                .iter()
                .map(|request| request.scaled_for_raster(raster_scale))
                .collect();
        }
        for metrics in &frame.scroll_metrics {
            self.interaction
                .controls
                .set_scroll_metrics(metrics.id.clone(), metrics.max_offset);
        }
        self.reconcile_focus(&frame);
        frame
    }

    /// Resolves every text request in frame order exactly once. The returned
    /// values are consumed by both the atlas and `UiSurfaceDrawList` so a
    /// dynamic label or text input cannot diverge between the two stages.
    pub fn resolve_text_requests<F>(&self, frame: &UiSurfaceFrame, mut resolve: F) -> Vec<String>
    where
        F: FnMut(&str) -> String,
    {
        frame
            .text_requests
            .iter()
            .map(|request| self.resolve_text_request(frame, request, |key| resolve(key)))
            .collect()
    }

    /// Synchronizes cached glyphs for values previously resolved by
    /// `resolve_text_requests`.
    pub fn sync_resolved_text(&mut self, frame: &UiSurfaceFrame, resolved_text: &[String]) {
        self.text_atlas.sync_resolved(
            frame
                .text_requests
                .iter()
                .zip(resolved_text.iter())
                .map(|(request, text)| (request, text.as_str())),
        );
    }

    /// Applies the measured atlas bounds to nodes authored with an intrinsic
    /// width or height. Localization is resolved before this pass, so a
    /// translated label no longer needs a hardcoded placeholder width.
    pub fn fit_intrinsic_text_to_resolved(
        &self,
        frame: &mut UiSurfaceFrame,
        resolved_text: &[String],
        raster_scale: f32,
    ) {
        let Some(bounds) = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.kind == UiNodeKind::Root)
            .map(|entry| entry.rect)
        else {
            return;
        };
        let raster_scale = raster_scale.clamp(1.0, 4.0);
        let max_width = (bounds.width - 8.0).max(1.0);
        let max_height = (bounds.height - 8.0).max(22.0);
        for (request_index, request) in frame.text_requests.iter().enumerate() {
            let Some(resolved) = resolved_text.get(request_index) else {
                continue;
            };
            let Some(layout_index) = frame
                .layout_boxes
                .iter()
                .position(|entry| entry.id == request.node_id)
            else {
                continue;
            };
            let layout = &frame.layout_boxes[layout_index];
            let left_inset = layout.content_rect.x - layout.rect.x;
            let top_inset = layout.content_rect.y - layout.rect.y;
            let right_inset = layout.rect.right() - layout.content_rect.right();
            let bottom_inset = layout.rect.bottom() - layout.content_rect.bottom();
            if layout.width_mode != UiSizeMode::FitContent
                && layout.width_mode != UiSizeMode::MinContent
                && layout.width_mode != UiSizeMode::MaxContent
                && layout.height_mode != UiSizeMode::FitContent
                && layout.height_mode != UiSizeMode::MinContent
                && layout.height_mode != UiSizeMode::MaxContent
            {
                continue;
            }
            let Some(slot) = self.text_atlas.slot_for(request, resolved.as_str()) else {
                continue;
            };
            let text_width = f32::from(slot.rect.width) / raster_scale;
            let text_height = f32::from(slot.rect.height) / raster_scale;
            let mut rect = layout.rect;
            if matches!(
                layout.width_mode,
                UiSizeMode::FitContent | UiSizeMode::MinContent | UiSizeMode::MaxContent
            ) {
                rect.width = (text_width + 8.0).clamp(1.0, max_width);
            }
            if matches!(
                layout.height_mode,
                UiSizeMode::FitContent | UiSizeMode::MinContent | UiSizeMode::MaxContent
            ) {
                rect.height = (text_height + 5.0).clamp(1.0, max_height);
            }
            rect = rect.clamp_inside(bounds.shrink(UiSpacing::same(4.0)));
            frame.layout_boxes[layout_index].rect = rect;
            frame.layout_boxes[layout_index].content_rect = UiRect::new(
                rect.x + left_inset,
                rect.y + top_inset,
                (rect.width - left_inset - right_inset).max(0.0),
                (rect.height - top_inset - bottom_inset).max(0.0),
            );
            if let Some(hit) = frame
                .hit_regions
                .iter_mut()
                .find(|region| region.id == request.node_id)
            {
                hit.rect = rect;
            }
        }
        self.reposition_tooltip(frame, bounds);
    }

    fn reposition_tooltip(&self, frame: &mut UiSurfaceFrame, viewport: UiRect) {
        let Some(hovered_id) = self.interaction.focus.hovered.as_deref() else {
            return;
        };
        let anchor = frame.tooltip_anchor.unwrap_or_else(|| {
            frame
                .layout_boxes
                .iter()
                .find(|layout| layout.id == hovered_id)
                .map(|layout| layout.rect)
                .unwrap_or(UiRect::new(viewport.x, viewport.y, 1.0, 1.0))
        });
        let Some(tooltip_index) = frame
            .layout_boxes
            .iter()
            .position(|layout| layout.id == "__rafui.tooltip")
        else {
            return;
        };
        let tooltip = frame.layout_boxes[tooltip_index].rect;
        let placement = raf_ui::place_overlay(
            anchor,
            [tooltip.width, tooltip.height],
            viewport.shrink(UiSpacing::same(4.0)),
            raf_ui::UiPlacement::BottomStart,
            8.0,
        );
        frame.layout_boxes[tooltip_index].rect = placement.rect;
        frame.layout_boxes[tooltip_index].content_rect = placement.rect;
    }

    /// Compatibility name for callers that only need the tooltip behavior.
    pub fn fit_tooltip_to_resolved_text(
        &self,
        frame: &mut UiSurfaceFrame,
        resolved_text: &[String],
        raster_scale: f32,
    ) {
        self.fit_intrinsic_text_to_resolved(frame, resolved_text, raster_scale);
    }

    /// Resolves a request exactly as the text atlas did for the current
    /// session. Presentation backends reuse this for text inputs so an empty
    /// field displays its placeholder instead of looking up an unrelated key.
    pub fn resolve_text_request<F>(
        &self,
        frame: &UiSurfaceFrame,
        request: &UiTextAtlasRequest,
        mut resolve: F,
    ) -> String
    where
        F: FnMut(&str) -> String,
    {
        let layout = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == request.node_id);
        if let Some(value) = layout.and_then(|layout| layout.text_value.as_deref()) {
            return value.to_string();
        }
        let input = layout.and_then(|layout| layout.control.text_input());
        match input {
            Some(input) if !self.interaction.controls.text(&input.value_key).is_empty() => {
                let value = self.interaction.controls.text(&input.value_key);
                if input.password {
                    "*".repeat(value.chars().count())
                } else {
                    value.to_string()
                }
            }
            Some(input) => input
                .placeholder_key
                .as_deref()
                .map(&mut resolve)
                .unwrap_or_default(),
            None => resolve(&request.text_key),
        }
    }

    pub fn process_input(
        &mut self,
        surface: &UiSurface,
        frame: &UiSurfaceFrame,
        input: &UiInputState,
    ) -> Vec<UiDispatchedAction> {
        self.tooltip_input_seen = true;
        self.reconcile_focus(frame);
        // Keep metric lookup renderer-owned while editing state and pointer
        // gesture ownership remain in RafUI. No control-state clone is needed
        // because RafUI passes the current value to this callback.
        let atlas = &self.text_atlas;
        let raster_scale = self.last_raster_scale.max(1.0);
        let actions = self.interaction.update_with_text_hit_test(
            &surface.root,
            &frame.hit_regions,
            input,
            &self.focus_policy,
            |node_id, value, point| {
                text_input_index_at(frame, atlas, raster_scale, node_id, value, point)
            },
        );
        let hovered = self.interaction.focus.hovered.clone();
        let hover_delay_elapsed = hovered.is_some()
            && self
                .interaction
                .hover_intent_progress(input.time_seconds, 0.32)
                >= 1.0;
        self.tooltip_motion
            .set_target(if hover_delay_elapsed { 1.0 } else { 0.0 });
        self.tooltip_motion.advance(
            input.delta_seconds_since(self.last_input_time_seconds),
            self.reduced_motion,
        );
        self.last_input_time_seconds = input.time_seconds;
        actions
    }

    fn reconcile_focus(&mut self, frame: &UiSurfaceFrame) {
        self.interaction.focus.blur_if_removed(
            frame
                .hit_regions
                .iter()
                .filter(|region| region.focusable && !region.disabled)
                .map(|region| region.id.as_str()),
        );
    }
}

fn text_input_index_at(
    frame: &UiSurfaceFrame,
    atlas: &UiTextAtlas,
    raster_scale: f32,
    node_id: &str,
    value: &str,
    point: [f32; 2],
) -> Option<usize> {
    let layout = frame
        .layout_boxes
        .iter()
        .find(|layout| layout.id == node_id)?;
    let input = layout.control.text_input()?;
    let request = frame
        .text_requests
        .iter()
        .find(|request| request.node_id == node_id)?;
    let rendered = if input.password {
        "*".repeat(value.chars().count())
    } else {
        value.to_string()
    };
    let length = rendered.chars().count();
    if length == 0 {
        return Some(0);
    }
    let icon_inset = layout
        .icon
        .map(|icon| 6.0 + f32::from(icon.size.logical_pixels()) + 6.0)
        .unwrap_or(0.0);
    let origin_x = layout.content_rect.x + icon_inset;
    let target = point[0] - origin_x;
    if target <= 0.0 {
        return Some(0);
    }
    let width_at =
        |cursor: usize| atlas.measure_prefix_width(request, &rendered, cursor) / raster_scale;
    let full_width = width_at(length);
    if target >= full_width {
        return Some(length);
    }

    // Prefix width is monotonic for the single-line text inputs used by the
    // retained editor. Binary search avoids measuring every character during
    // a drag, which matters for long prompts and settings values.
    let mut low = 0;
    let mut high = length;
    while low < high {
        let middle = low + (high - low) / 2;
        if width_at(middle) < target {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let right = low;
    let left = right.saturating_sub(1);
    let cursor = if target - width_at(left) <= width_at(right) - target {
        left
    } else {
        right
    };
    Some(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_layout_distributes_grow_child() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_style(palette.root_style())
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                padding: UiSpacing::same(8.0),
                gap: 4.0,
                ..UiLayout::fill(UiFlow::Column)
            })
            .with_child(
                UiNode::new("toolbar", UiNodeKind::Toolbar)
                    .with_layout(UiLayout::fixed(0.0, 32.0))
                    .with_style(palette.panel_style()),
            )
            .with_child(
                UiNode::new("canvas", UiNodeKind::Canvas)
                    .with_layout(UiLayout {
                        grow: 1.0,
                        ..UiLayout::default()
                    })
                    .with_style(palette.panel_style())
                    .interactive(),
            );
        let surface = UiSurface::new("test", palette, root);
        let frame = surface.build_frame(320, 200, [0, 0, 0, 255]);

        let canvas = frame
            .layout_boxes
            .iter()
            .find(|layout_box| layout_box.id == "canvas")
            .expect("canvas layout box");
        assert_eq!(canvas.rect.x, 8.0);
        assert_eq!(canvas.rect.y, 44.0);
        assert_eq!(canvas.rect.width, 304.0);
        assert_eq!(canvas.rect.height, 148.0);
        assert_eq!(
            surface
                .hit_test(&frame.layout_boxes, [20.0, 60.0])
                .map(|hit| hit.id.as_str()),
            Some("canvas")
        );
    }

    #[test]
    fn session_uses_text_metrics_for_pointer_caret_positions() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::text_input("query", UiTextInput::new("query.value"))
                .with_layout(UiLayout::fixed(220.0, 28.0))
                .focusable(),
        );
        let surface = UiSurface::new("text-hit-test", StudioUiPalette::IndustrialDark, root);
        let mut session = UiSurfaceSession::default();
        session
            .interaction
            .controls
            .set_text("query.value", "WW iii", 64);
        let frame = session
            .build_frame_with_resolved_text(&surface, 260, 80, [0; 4], |key| key.to_string());
        let layout = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "query")
            .expect("text input layout");
        let request = frame
            .text_requests
            .iter()
            .find(|request| request.node_id == "query")
            .expect("text input request");
        let x = layout.content_rect.x
            + session
                .text_atlas
                .measure_prefix_width(request, "WW iii", 2);
        session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some([x, layout.content_rect.y + 12.0]),
                pointer_down: true,
                time_seconds: 1.0,
                ..UiInputState::default()
            },
        );

        assert_eq!(
            session.interaction.controls.text_edit("query.value").cursor,
            2
        );
    }

    #[test]
    fn session_caches_surface_text_requests() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("title", UiNodeKind::Label)
                .with_text_key("app.project_settings")
                .with_text_style(UiTextStyle::panel_title([255, 255, 255, 255])),
        );
        let surface = UiSurface::new("session", palette, root);
        let mut session = UiSurfaceSession::default();

        session.build_frame(&surface, 320, 200, [0, 0, 0, 255]);

        assert_eq!(session.text_atlas.slot_count(), 1);
    }

    #[test]
    fn surface_updates_preserve_existing_text_atlas_slots() {
        let palette = StudioUiPalette::IndustrialDark;
        let mut session = UiSurfaceSession::default();
        let first = UiSurface::new(
            "agent",
            palette,
            UiNode::new("root", UiNodeKind::Root).with_child(
                UiNode::new("message", UiNodeKind::Label)
                    .with_text_value("You")
                    .with_text_style(UiTextStyle::body([255, 255, 255, 255])),
            ),
        );
        let first_frame = session.build_layout_frame_at_scale(&first, 320, 120, [0; 4], 1.0);
        let first_text = session.resolve_text_requests(&first_frame, |_| String::new());
        let first_request = first_frame.text_requests[0].clone();
        session.sync_resolved_text(&first_frame, &first_text);
        assert!(session.text_atlas.slot_for(&first_request, "You").is_some());

        let second = UiSurface::new(
            "agent",
            palette,
            UiNode::new("root", UiNodeKind::Root).with_child(
                UiNode::new("message", UiNodeKind::Label)
                    .with_text_value("Agent")
                    .with_text_style(UiTextStyle::body([255, 255, 255, 255])),
            ),
        );
        let second_frame = session.build_layout_frame_at_scale(&second, 320, 120, [0; 4], 1.0);
        let second_text = session.resolve_text_requests(&second_frame, |_| String::new());
        session.sync_resolved_text(&second_frame, &second_text);

        assert!(session.text_atlas.slot_for(&first_request, "You").is_some());
        assert!(session
            .text_atlas
            .slot_for(&second_frame.text_requests[0], "Agent")
            .is_some());
    }

    #[test]
    fn draw_list_uses_the_same_placeholder_resolution_as_the_text_atlas() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::text_input(
                "search",
                UiTextInput {
                    value_key: "search.value".to_string(),
                    placeholder_key: Some("search.placeholder".to_string()),
                    ..UiTextInput::new("search.value")
                },
            )
            .with_layout(UiLayout::fixed(240.0, 32.0)),
        );
        let surface = UiSurface::new("placeholder", StudioUiPalette::IndustrialDark, root);
        let mut session = UiSurfaceSession::default();
        let resolve = |key: &str| match key {
            "search.placeholder" => "Search projects".to_string(),
            _ => key.to_string(),
        };
        let frame =
            session.build_frame_with_resolved_text(&surface, 320, 80, [0, 0, 0, 255], resolve);
        let draw_list =
            UiSurfaceDrawList::build_with_resolved_text(&frame, &session.text_atlas, |request| {
                session.resolve_text_request(&frame, request, resolve)
            });

        assert_eq!(draw_list.text.len(), 1);
    }

    #[test]
    fn literal_text_bypasses_translation_fallbacks() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("value", UiNodeKind::Label)
                .with_text_value("42 px | 50%")
                .with_layout(UiLayout::fit_content()),
        );
        let surface = UiSurface::new("literal-text", StudioUiPalette::IndustrialDark, root);
        let mut session = UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 320, 80, [0, 0, 0, 255], |_| {
                "unexpected translation fallback".to_string()
            });
        let resolved = session.resolve_text_requests(&frame, |_| "unexpected".to_string());

        assert_eq!(resolved, vec!["42 px | 50%".to_string()]);
    }

    #[test]
    fn layout_honors_minimum_and_maximum_sizes_in_flow() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                ..UiLayout::fill(UiFlow::Row)
            })
            .with_child(
                UiNode::new("minimum", UiNodeKind::Panel).with_layout(UiLayout {
                    basis: [20.0, 0.0],
                    min_size: [96.0, 32.0],
                    ..UiLayout::default()
                }),
            )
            .with_child(
                UiNode::new("maximum", UiNodeKind::Panel).with_layout(UiLayout {
                    basis: [180.0, 0.0],
                    max_size: [72.0, 0.0],
                    ..UiLayout::default()
                }),
            );
        let surface = UiSurface::new("bounds", palette, root);
        let frame = surface.build_frame(320, 120, [0, 0, 0, 255]);

        let minimum = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "minimum")
            .unwrap();
        let maximum = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "maximum")
            .unwrap();
        assert_eq!(minimum.rect.width, 96.0);
        assert_eq!(maximum.rect.width, 72.0);
    }

    #[test]
    fn crowded_flow_compresses_without_sibling_overlap() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 8.0,
                ..UiLayout::fill(UiFlow::Row)
            })
            .with_child(
                UiNode::new("first", UiNodeKind::Panel).with_layout(UiLayout {
                    min_size: [120.0, 20.0],
                    ..UiLayout::default()
                }),
            )
            .with_child(
                UiNode::new("second", UiNodeKind::Panel).with_layout(UiLayout {
                    min_size: [120.0, 20.0],
                    ..UiLayout::default()
                }),
            );
        let surface = UiSurface::new("crowded", palette, root);
        let frame = surface.build_frame(180, 80, [0, 0, 0, 255]);
        let first = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "first")
            .unwrap();
        let second = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "second")
            .unwrap();

        assert!(first.rect.right() <= second.rect.x);
        assert!(second.rect.right() <= 180.0);
    }

    #[test]
    fn session_applies_hover_style_to_the_next_retained_frame() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("save", UiNodeKind::Button)
                .with_class("primary")
                .with_layout(UiLayout::fixed(80.0, 28.0)),
        );
        let mut surface = UiSurface::new("hover", palette, root);
        surface.style_sheet.rules = vec![
            UiStyleRule::new(
                UiStyleSelector::Class("primary".to_string()),
                UiStylePatch {
                    fill: Some([224, 116, 24, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("primary".to_string()),
                UiStylePatch {
                    fill: Some([255, 151, 46, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ];
        let mut session = UiSurfaceSession::default();
        session
            .interaction
            .focus
            .set_hovered(Some("save".to_string()));

        let frame = session.build_frame(&surface, 240, 120, [0, 0, 0, 255]);
        let save = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "save")
            .unwrap();

        assert_eq!(save.style.fill, [255, 151, 46, 255]);
    }

    #[test]
    fn hovered_control_adds_a_compact_clipped_tooltip_to_the_retained_frame() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("save", UiNodeKind::Button)
                .with_tooltip_key("app.save")
                .with_layout(UiLayout::fixed(80.0, 28.0))
                .focusable(),
        );
        let surface = UiSurface::new("tooltip", palette, root);
        let mut session = UiSurfaceSession::default();
        session.interaction.focus.request_focus("save");
        let focused_frame = session.build_frame(&surface, 160, 80, [0, 0, 0, 255]);
        assert!(!focused_frame
            .layout_boxes
            .iter()
            .any(|entry| entry.kind == UiNodeKind::Tooltip));

        session
            .interaction
            .focus
            .set_hovered(Some("save".to_string()));

        let frame = session.build_frame(&surface, 160, 80, [0, 0, 0, 255]);
        let tooltip = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.kind == UiNodeKind::Tooltip)
            .expect("focused tooltip layout box");

        assert_eq!(tooltip.text_key.as_deref(), Some("app.save"));
        assert!(tooltip.rect.width <= 238.0);
        assert_eq!(tooltip.text_style.as_ref().unwrap().color[3], 255);
        assert!(tooltip.rect.right() <= 160.0);
        assert!(tooltip.rect.bottom() <= 80.0);
        assert!(frame
            .text_requests
            .iter()
            .any(|request| request.node_id == "__rafui.tooltip"));

        let fitted_frame =
            session.build_frame_with_resolved_text(&surface, 320, 80, [0, 0, 0, 255], |_| {
                "Save".to_string()
            });
        let fitted_tooltip = fitted_frame
            .layout_boxes
            .iter()
            .find(|entry| entry.kind == UiNodeKind::Tooltip)
            .expect("fitted tooltip layout box");
        assert!(fitted_tooltip.rect.width < 120.0);
    }

    #[test]
    fn literal_tooltip_value_is_rendered_without_i18n_resolution() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("metadata", UiNodeKind::Label)
                .with_tooltip_value("World_Rafi")
                .with_layout(UiLayout::fixed(96.0, 24.0))
                .interactive(),
        );
        let surface = UiSurface::new("literal-tooltip", StudioUiPalette::IndustrialDark, root);
        let mut session = UiSurfaceSession::default();
        session
            .interaction
            .focus
            .set_hovered(Some("metadata".to_string()));

        let frame = session.build_frame(&surface, 180, 80, [0, 0, 0, 255]);
        let tooltip = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.kind == UiNodeKind::Tooltip)
            .expect("literal tooltip layout box");

        assert_eq!(tooltip.text_value.as_deref(), Some("World_Rafi"));
        assert!(tooltip.text_key.is_none());
        assert!(frame.text_requests.iter().any(
            |request| request.node_id == "__rafui.tooltip" && request.text_key == "World_Rafi"
        ));
    }

    #[test]
    fn tooltip_waits_for_hover_intent_and_anchors_to_the_pointer() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("grid", UiNodeKind::Button)
                .with_layout(UiLayout::absolute(UiRect::new(12.0, 12.0, 32.0, 32.0)))
                .with_tooltip_key("viewport.grid")
                .interactive(),
        );
        let surface = UiSurface::new("tooltip-delay", StudioUiPalette::IndustrialDark, root);
        let mut session = UiSurfaceSession::default();
        let mut frame = session.build_layout_frame_at_scale(&surface, 160, 100, [0; 4], 1.0);

        session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some([21.0, 31.0]),
                time_seconds: 0.0,
                ..UiInputState::default()
            },
        );
        frame = session.build_layout_frame_at_scale(&surface, 160, 100, [0; 4], 1.0);
        assert!(!frame
            .layout_boxes
            .iter()
            .any(|layout| layout.kind == UiNodeKind::Tooltip));

        session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some([21.0, 31.0]),
                time_seconds: 0.20,
                ..UiInputState::default()
            },
        );
        frame = session.build_layout_frame_at_scale(&surface, 160, 100, [0; 4], 1.0);
        assert!(!frame
            .layout_boxes
            .iter()
            .any(|layout| layout.kind == UiNodeKind::Tooltip));

        session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some([21.0, 31.0]),
                time_seconds: 0.34,
                ..UiInputState::default()
            },
        );
        frame = session.build_layout_frame_at_scale(&surface, 160, 100, [0; 4], 1.0);
        let tooltip = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.kind == UiNodeKind::Tooltip)
            .expect("tooltip after hover intent delay");
        assert!(tooltip.rect.y >= 31.0);
        assert_eq!(
            frame.tooltip_anchor,
            Some(UiRect::new(21.0, 31.0, 1.0, 1.0))
        );
    }

    #[test]
    fn child_of_an_elevated_overlay_wins_hit_testing_over_base_canvas() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_child(
                UiNode::new("canvas", UiNodeKind::Canvas)
                    .with_layout(UiLayout::absolute(UiRect::new(0.0, 0.0, 240.0, 120.0)))
                    .interactive(),
            )
            .with_child(
                UiNode::new("menu-overlay", UiNodeKind::Overlay)
                    .with_layout(
                        UiLayout::absolute(UiRect::new(20.0, 20.0, 120.0, 70.0)).with_z_index(20),
                    )
                    .with_child(
                        UiNode::new("menu-action", UiNodeKind::Button)
                            .with_layout(UiLayout::fill(UiFlow::None))
                            .interactive(),
                    ),
            );
        let surface = UiSurface::new("overlay", palette, root);
        let frame = surface.build_frame(240, 120, [0, 0, 0, 255]);

        let hit = surface
            .hit_test_regions(&frame.hit_regions, [40.0, 40.0])
            .expect("overlay hit");
        assert_eq!(hit.id, "menu-action");
        assert_eq!(hit.z_index, 20);
    }

    #[test]
    fn compact_row_stacks_without_overlapping_when_space_runs_out() {
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                compact: UiCompactMode::Stack,
                gap: 8.0,
                ..UiLayout::fill(UiFlow::Row)
            })
            .with_child(
                UiNode::new("first", UiNodeKind::Panel).with_layout(UiLayout::fixed(120.0, 32.0)),
            )
            .with_child(
                UiNode::new("second", UiNodeKind::Panel).with_layout(UiLayout::fixed(120.0, 32.0)),
            );
        let surface = UiSurface::new("compact", StudioUiPalette::IndustrialDark, root);
        let frame = surface.build_frame(160, 120, [0, 0, 0, 255]);
        let first = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "first")
            .unwrap();
        let second = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "second")
            .unwrap();

        assert!(second.rect.y >= first.rect.bottom());
    }

    #[test]
    fn automatic_grid_uses_multiple_columns_when_the_surface_allows_it() {
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout {
                flow: UiFlow::Grid,
                gap: 10.0,
                grid: UiGridLayout {
                    columns: 0,
                    min_column_width: 180.0,
                    row_height: 48.0,
                },
                ..UiLayout::fill(UiFlow::Grid)
            })
            .with_child(UiNode::new("one", UiNodeKind::Panel))
            .with_child(UiNode::new("two", UiNodeKind::Panel));
        let surface = UiSurface::new("grid", StudioUiPalette::IndustrialDark, root);
        let frame = surface.build_frame(420, 120, [0, 0, 0, 255]);
        let one = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "one")
            .unwrap();
        let two = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "two")
            .unwrap();

        assert_eq!(one.rect.y, two.rect.y);
        assert!(two.rect.x > one.rect.x);
    }
}
