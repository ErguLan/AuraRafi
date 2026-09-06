//! Reusable orientation compass for the native Game viewport.
//!
//! The compass is intentionally split from the workbench surface. Its model
//! owns the camera-to-screen projection and its host owns only the retained
//! RafUI presentation/input boundary. Camera mutation stays in the viewport
//! controller, so future compass settings can change presentation without
//! coupling the widget to scene or renderer state.

use glam::Vec3;
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette,
    UiAccessibilityRole, UiAlign, UiDispatchedAction, UiEventBinding, UiEventKind, UiFlow,
    UiFontWeight, UiJustify, UiLayout, UiNode, UiNodeKind, UiRect, UiSpacing, UiStyle,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface,
    UiTextOverflow, UiTextRole, UiTextStyle,
};
use raf_render::api_graphic_basic::EditorUiLayer;

use crate::editor_layout::EditorRect;

const COMPASS_SURFACE_ID: &str = "editor.viewport.compass";
const COMPASS_ROOT_ID: &str = "viewport.compass.root";
const COMPASS_BACKGROUND_ID: &str = "viewport.compass.background";
const COMPASS_CENTER_ID: &str = "viewport.compass.center";
const COMPASS_AXIS_CLASS: &str = "viewport-compass-axis";
// Visual defaults tuned for legibility on a clean overlay panel: a 76px
// square with enough internal padding to keep axis handles, the ISO reset
// and the axis labels from competing for the same pixels. The axis reach is
// fixed at 22px so the endpoint buttons always sit inside the panel even
// when the orbit tilts an axis toward the corner.
const COMPASS_DEFAULT_SIZE: f32 = 76.0;
const COMPASS_DEFAULT_MARGIN: f32 = 14.0;
const COMPASS_MIN_SIZE: f32 = 56.0;
const COMPASS_PANEL_RADIUS_FACTOR: f32 = 0.16;
const COMPASS_INSET_FACTOR: f32 = 0.12;
const COMPASS_CENTER_RADIUS_FACTOR: f32 = 0.135;
const COMPASS_ENDPOINT_SIZE_FACTOR: f32 = 0.18;
const COMPASS_ENDPOINT_DOT_FACTOR: f32 = 0.11;
const COMPASS_SHAFT_DOT_SPACING: f32 = 3.0;
const COMPASS_SHAFT_DOT_SIZE_FACTOR: f32 = 0.04;
const COMPASS_LABEL_GAP_FACTOR: f32 = 0.04;

/// Presentation knobs reserved for the future compass settings surface.
///
/// Keeping these values together means a later settings panel can update the
/// widget through `ViewportCompassHost::set_config` without changing camera
/// math or the workbench composition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportCompassConfig {
    pub size: f32,
    pub margin: f32,
    pub axis_length: f32,
    pub hit_radius: f32,
    pub min_size: f32,
}

impl Default for ViewportCompassConfig {
    fn default() -> Self {
        Self {
            size: COMPASS_DEFAULT_SIZE,
            margin: COMPASS_DEFAULT_MARGIN,
            axis_length: 22.0,
            hit_radius: 6.5,
            min_size: COMPASS_MIN_SIZE,
        }
    }
}

impl ViewportCompassConfig {
    fn normalized(self) -> Self {
        let size = self.size.max(self.min_size.max(40.0));
        let hit_radius = self.hit_radius.clamp(5.0, (size * 0.18).max(5.0));
        let endpoint_radius = compass_endpoint_size(size, hit_radius) * 0.5;
        let max_axis_length = (size * 0.5 - endpoint_radius - 2.0).max(8.0);
        Self {
            size,
            margin: self.margin.max(0.0),
            axis_length: self.axis_length.clamp(8.0, max_axis_length),
            hit_radius,
            min_size: self.min_size.clamp(40.0, size),
        }
    }
}

/// Current camera orientation consumed by the compass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportCompassState {
    pub yaw: f32,
    pub pitch: f32,
    pub visible: bool,
}

impl Default for ViewportCompassState {
    fn default() -> Self {
        Self {
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 0.5,
            visible: true,
        }
    }
}

impl ViewportCompassState {
    pub fn from_orbit(yaw: f32, pitch: f32) -> Self {
        Self {
            yaw,
            pitch,
            visible: true,
        }
    }

    pub const fn hidden() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            visible: false,
        }
    }

    fn sanitized(self) -> Self {
        Self {
            yaw: if self.yaw.is_finite() { self.yaw } else { 0.0 },
            pitch: if self.pitch.is_finite() {
                self.pitch.clamp(-1.52, 1.52)
            } else {
                0.0
            },
            visible: self.visible,
        }
    }
}

/// Camera targets supported by the current three-axis compass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportCompassTarget {
    X,
    Y,
    Z,
    Isometric,
}

impl ViewportCompassTarget {
    pub fn from_command(command: &str) -> Option<Self> {
        match command {
            "viewport.compass.snap:x" => Some(Self::X),
            "viewport.compass.snap:y" => Some(Self::Y),
            "viewport.compass.snap:z" => Some(Self::Z),
            "viewport.compass.reset" => Some(Self::Isometric),
            _ => None,
        }
    }

    pub const fn axis(self) -> Option<Vec3> {
        match self {
            Self::X => Some(Vec3::X),
            Self::Y => Some(Vec3::Y),
            Self::Z => Some(Vec3::Z),
            Self::Isometric => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportCompassAxisProjection {
    pub position: [f32; 2],
    /// Positive values point toward the camera and are rendered more strongly.
    pub depth: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportCompassLayout {
    pub size: f32,
    pub center: [f32; 2],
    pub center_radius: f32,
    pub panel_radius: f32,
    pub panel_inset: f32,
    pub axis_points: [ViewportCompassAxisProjection; 3],
    pub endpoint_size: f32,
    pub endpoint_dot: f32,
    pub shaft_dot_size: f32,
    pub shaft_dot_spacing: f32,
    pub label_size: [f32; 2],
    pub label_gap: f32,
}

impl ViewportCompassLayout {
    pub fn new(state: ViewportCompassState, config: ViewportCompassConfig) -> Self {
        let config = config.normalized();
        let state = state.sanitized();
        let center = [config.size * 0.5, config.size * 0.5];
        let center_radius = config.size * COMPASS_CENTER_RADIUS_FACTOR;
        let panel_radius = (config.size * COMPASS_PANEL_RADIUS_FACTOR).clamp(8.0, 14.0);
        let panel_inset = (config.size * COMPASS_INSET_FACTOR).clamp(4.0, 8.0);
        let endpoint_size = compass_endpoint_size(config.size, config.hit_radius);
        let endpoint_dot = (config.size * COMPASS_ENDPOINT_DOT_FACTOR).clamp(6.0, 10.0);
        let shaft_dot_size = (config.size * COMPASS_SHAFT_DOT_SIZE_FACTOR).clamp(1.5, 2.25);
        let shaft_dot_spacing = COMPASS_SHAFT_DOT_SPACING;
        let label_size = [
            (config.size * 0.20).clamp(11.0, 15.0),
            (config.size * 0.20).clamp(11.0, 15.0),
        ];
        let label_gap = (config.size * COMPASS_LABEL_GAP_FACTOR).clamp(2.0, 4.0);
        let (right, up, camera_back) = screen_basis(state);
        let axes = [Vec3::X, Vec3::Y, Vec3::Z];
        let axis_points = axes.map(|axis| {
            let screen = [axis.dot(right), axis.dot(up)];
            ViewportCompassAxisProjection {
                position: [
                    center[0] + screen[0] * config.axis_length,
                    center[1] - screen[1] * config.axis_length,
                ],
                depth: axis.dot(camera_back),
            }
        });
        Self {
            size: config.size,
            center,
            center_radius,
            panel_radius,
            panel_inset,
            axis_points,
            endpoint_size,
            endpoint_dot,
            shaft_dot_size,
            shaft_dot_spacing,
            label_size,
            label_gap,
        }
    }
}

fn screen_basis(state: ViewportCompassState) -> (Vec3, Vec3, Vec3) {
    let state = state.sanitized();
    let yaw = state.yaw;
    let pitch = state.pitch;
    // This is the same orbit convention used by ViewportBridge: yaw zero
    // places the camera on +Z and positive pitch raises it above the target.
    let camera_back = Vec3::new(
        pitch.cos() * yaw.sin(),
        pitch.sin(),
        pitch.cos() * yaw.cos(),
    )
    .normalize_or_zero();
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin()).normalize_or_zero();
    let up = right.cross(-camera_back).normalize_or_zero();
    (right, up, camera_back)
}

/// Returns a logical rect anchored to the lower-left of the viewport.
pub fn compass_rect_for_viewport(
    viewport: EditorRect,
    config: ViewportCompassConfig,
) -> Option<EditorRect> {
    let config = config.normalized();
    let available_width = viewport.width - config.margin * 2.0;
    let available_height = viewport.height - config.margin * 2.0;
    if available_width < config.min_size || available_height < config.min_size {
        return None;
    }
    let size = config.size.min(available_width).min(available_height);
    Some(EditorRect::new(
        viewport.x + config.margin,
        viewport.y + viewport.height - config.margin - size,
        size,
        size,
    ))
}

/// Retained surface for the compass, useful to tests and to other native
/// viewport hosts that may want the same orientation widget later.
pub fn build_viewport_compass_surface(
    palette: StudioUiPalette,
    state: ViewportCompassState,
    config: ViewportCompassConfig,
) -> UiSurface {
    let config = config.normalized();
    let layout = ViewportCompassLayout::new(state, config);
    let tokens = palette.tokens();
    let (panel_fill, panel_border, iso_text, _label_halo) = compass_panel_palette(palette);
    let shaft_dot_size = (layout.size * 0.022).clamp(1.5, 2.25);
    let shaft_dot_spacing = (shaft_dot_size * 0.9).max(1.4);
    let endpoint_size = layout.endpoint_size;
    let endpoint_dot = layout.endpoint_dot;
    let label_size = layout.label_size;
    let label_gap = layout.label_gap;
    let panel_inset = layout.panel_inset;
    let panel_radius = layout.panel_radius;

    let mut root = UiNode::new(COMPASS_ROOT_ID, UiNodeKind::Root)
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new(COMPASS_BACKGROUND_ID, UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(UiRect::new(
                    0.0,
                    0.0,
                    layout.size,
                    layout.size,
                )))
                .with_style(UiStyle {
                    fill: panel_fill,
                    border: opaque(panel_border),
                    text: iso_text,
                    border_width: 1.0,
                    radius: panel_radius,
                    opacity: 0.1,
                }),
        );

    for (index, axis) in [CompassAxis::X, CompassAxis::Y, CompassAxis::Z]
        .into_iter()
        .enumerate()
    {
        let endpoint = layout.axis_points[index];
        let color = axis_color(axis);
        let line_color = opaque(color);
        let dx = endpoint.position[0] - layout.center[0];
        let dy = endpoint.position[1] - layout.center[1];
        let distance = (dx * dx + dy * dy).sqrt();
        let inner_radius = endpoint_dot * 0.45;
        let outer_radius = endpoint_dot * 0.6;
        let inner_sq = inner_radius * inner_radius;
        let outer_sq = outer_radius * outer_radius;
        let shaft_dot_count = ((distance - outer_radius) / shaft_dot_spacing).floor() as usize;
        for dot in 0..shaft_dot_count {
            let fraction = (dot as f32 + 0.5) / shaft_dot_count.max(1) as f32;
            let x = layout.center[0] + dx * fraction;
            let y = layout.center[1] + dy * fraction;
            let dist_from_center_sq =
                (x - layout.center[0]).powi(2) + (y - layout.center[1]).powi(2);
            if dist_from_center_sq < inner_sq {
                continue;
            }
            let dist_from_endpoint_sq =
                (x - endpoint.position[0]).powi(2) + (y - endpoint.position[1]).powi(2);
            if dist_from_endpoint_sq < outer_sq {
                continue;
            }
            root = root.with_child(
                UiNode::new(
                    format!("viewport.compass.axis.{}.shaft.{dot}", axis.slug()),
                    UiNodeKind::Panel,
                )
                .with_layout(
                    UiLayout::absolute(UiRect::new(
                        x - shaft_dot_size * 0.5,
                        y - shaft_dot_size * 0.5,
                        shaft_dot_size,
                        shaft_dot_size,
                    ))
                    .with_z_index(10),
                )
                .with_style(UiStyle {
                    fill: line_color,
                    border: line_color,
                    text: line_color,
                    border_width: 0.0,
                    radius: shaft_dot_size * 0.5,
                    opacity: 1.0,
                }),
            );
        }

        let command = axis.command();
        let tooltip = axis.tooltip_key();
        let hit_rect = UiRect::new(
            endpoint.position[0] - endpoint_size * 0.5,
            endpoint.position[1] - endpoint_size * 0.5,
            endpoint_size,
            endpoint_size,
        );
        let hit_node = UiNode::new(
            format!("viewport.compass.axis.{}.hit", axis.slug()),
            UiNodeKind::Panel,
        )
        .with_class(COMPASS_AXIS_CLASS)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            padding: UiSpacing::ZERO,
            ..UiLayout::absolute(hit_rect).with_z_index(25)
        })
        .with_style(UiStyle {
            fill: opaque(color),
            border: opaque(color),
            text: tokens.text,
            border_width: 0.0,
            radius: hit_rect.width * 0.5,
            opacity: 0.0,
        })
        .with_tooltip_key(tooltip)
        .with_accessibility_label_key(tooltip)
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command));
        root = root.with_child(hit_node);

        let endpoint_button = UiNode::new(
            format!("viewport.compass.axis.{}", axis.slug()),
            UiNodeKind::Panel,
        )
        .with_layout(
            UiLayout::absolute(UiRect::new(
                endpoint.position[0] - endpoint_dot * 0.5,
                endpoint.position[1] - endpoint_dot * 0.5,
                endpoint_dot,
                endpoint_dot,
            ))
            .with_z_index(30),
        )
        .with_style(UiStyle {
            fill: opaque(color),
            border: opaque(mix_rgba(color, [255, 255, 255, 255], 0.25)),
            text: tokens.text,
            border_width: 1.0,
            radius: endpoint_dot * 0.5,
            opacity: 1.0,
        });
        root = root.with_child(endpoint_button);

        let label_width = label_size[0];
        let label_height = label_size[1];
        let vertical = dy.abs() >= dx.abs();
        let raw_x = if vertical {
            endpoint.position[0] - label_width * 0.5
        } else if dx >= 0.0 {
            endpoint.position[0] + endpoint_dot * 0.5 + label_gap
        } else {
            endpoint.position[0] - endpoint_dot * 0.5 - label_gap - label_width
        };
        let raw_y = if vertical {
            if dy < 0.0 {
                endpoint.position[1] - endpoint_dot * 0.5 - label_gap - label_height
            } else {
                endpoint.position[1] + endpoint_dot * 0.5 + label_gap
            }
        } else {
            endpoint.position[1] - label_height * 0.5
        };
        let label_x = raw_x.clamp(
            panel_inset,
            (layout.size - label_width - panel_inset).max(panel_inset),
        );
        let label_y = raw_y.clamp(
            panel_inset,
            (layout.size - label_height - panel_inset).max(panel_inset),
        );
        root = root.with_child(
            UiNode::new(
                format!("viewport.compass.axis.{}.label", axis.slug()),
                UiNodeKind::Label,
            )
            .with_layout(
                UiLayout::absolute(UiRect::new(label_x, label_y, label_width, label_height))
                    .with_z_index(35),
            )
            .with_style(UiStyle::transparent())
            .with_text_value(axis.label())
            .with_text_style(UiTextStyle {
                role: UiTextRole::Label,
                size_px: (label_height * 0.78).clamp(9.0, 12.0),
                line_height_px: label_height,
                weight: UiFontWeight::Bold,
                color,
                inherit_color: false,
            })
            .with_text_overflow(UiTextOverflow::Clip),
        );
    }

    let iso_radius = layout.center_radius;
    let iso_button = UiNode::new(COMPASS_CENTER_ID, UiNodeKind::Button)
        .with_class("viewport-compass-center")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            padding: UiSpacing::ZERO,
            ..UiLayout::absolute(UiRect::new(
                layout.center[0] - iso_radius,
                layout.center[1] - iso_radius,
                iso_radius * 2.0,
                iso_radius * 2.0,
            ))
            .with_z_index(40)
        })
        .with_style(UiStyle {
            fill: [0, 0, 0, 0],
            border: opaque(tokens.accent),
            text: iso_text,
            border_width: 1.0,
            radius: iso_radius,
            opacity: 1.0,
        })
        .with_text_value("ISO")
        .with_text_style(UiTextStyle {
            role: UiTextRole::Button,
            size_px: (iso_radius * 0.95).clamp(9.0, 12.0),
            line_height_px: (iso_radius * 1.5).clamp(11.0, 15.0),
            weight: UiFontWeight::Bold,
            color: iso_text,
            inherit_color: false,
        })
        .with_text_overflow(UiTextOverflow::Clip)
        .with_tooltip_key("viewport.hud.reset_iso")
        .with_accessibility_label_key("viewport.hud.reset_iso")
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "viewport.compass.reset",
        ));
    root = root.with_child(iso_button);

    let mut surface = UiSurface::new(COMPASS_SURFACE_ID, palette, root);
    surface.style_sheet = compass_style_sheet(palette);
    surface
}

fn compass_endpoint_size(size: f32, hit_radius: f32) -> f32 {
    (hit_radius * 2.0).clamp(12.0, (size * COMPASS_ENDPOINT_SIZE_FACTOR).max(12.0))
}

fn opaque(mut color: [u8; 4]) -> [u8; 4] {
    color[3] = 255;
    color
}

fn mix_rgba(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t).round() as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t).round() as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t).round() as u8,
        a[3],
    ]
}

fn compass_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let ring = opaque(tokens.focus);
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class(COMPASS_AXIS_CLASS.to_string()),
                UiStylePatch {
                    border: Some(ring),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class(COMPASS_AXIS_CLASS.to_string()),
                UiStylePatch {
                    border: Some(ring),
                    border_width: Some(1.5),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("viewport-compass-center".to_string()),
                UiStylePatch {
                    border: Some(ring),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("viewport-compass-center".to_string()),
                UiStylePatch {
                    border: Some(ring),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
        ],
    }
}

/// Returns the visual palette for the compass panel: a translucent card whose
/// lightness flips with the active editor theme so the widget always reads
/// against the viewport. Opacity stays intentionally low (around 40%) so the
/// scene shows through, while the border and label halo provide enough
/// contrast to keep axes, labels and the ISO reset legible on light or dark
/// backgrounds. The card stays intentional, not decorative: translucent so
/// the viewport shows through, opaque enough to keep axis handles, labels,
/// and the ISO reset readable.
fn compass_panel_palette(palette: StudioUiPalette) -> ([u8; 4], [u8; 4], [u8; 4], [u8; 4]) {
    let tokens = palette.tokens();
    if palette_is_light(palette) {
        (
            [24, 28, 36, 220],
            mix_rgba(tokens.text, [255, 255, 255, 255], 0.6),
            [255, 255, 255, 255],
            [0, 0, 0, 0],
        )
    } else {
        (
            [255, 255, 255, 220],
            mix_rgba(tokens.text, [0, 0, 0, 255], 0.45),
            [20, 24, 32, 255],
            [0, 0, 0, 0],
        )
    }
}

fn palette_is_light(palette: StudioUiPalette) -> bool {
    let tokens = palette.tokens();
    let luminance = 0.2126 * tokens.surface[0] as f32
        + 0.7152 * tokens.surface[1] as f32
        + 0.0722 * tokens.surface[2] as f32;
    luminance > 128.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompassAxis {
    X,
    Y,
    Z,
}

impl CompassAxis {
    const fn label(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
        }
    }

    const fn slug(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }

    const fn command(self) -> &'static str {
        match self {
            Self::X => "viewport.compass.snap:x",
            Self::Y => "viewport.compass.snap:y",
            Self::Z => "viewport.compass.snap:z",
        }
    }

    const fn tooltip_key(self) -> &'static str {
        match self {
            Self::X => "viewport.hud.snap_x",
            Self::Y => "viewport.hud.snap_y",
            Self::Z => "viewport.hud.snap_z",
        }
    }
}

fn axis_color(axis: CompassAxis) -> [u8; 4] {
    match axis {
        // Keep the classic viewport HUD colors instead of inheriting muted
        // status colors from the editor palette.
        CompassAxis::X => [220, 70, 70, 255],
        CompassAxis::Y => [70, 220, 70, 255],
        CompassAxis::Z => [70, 100, 220, 255],
    }
}

/// Native retained host for the compass layer.
pub struct ViewportCompassHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    config: ViewportCompassConfig,
    active: bool,
    last_sync: Option<(
        StudioUiPalette,
        EditorRect,
        ViewportCompassState,
        ViewportCompassConfig,
        bool,
    )>,
}

impl ViewportCompassHost {
    pub fn new(graphics: &NativeGraphicsContext<'_>, palette: StudioUiPalette) -> Self {
        let config = ViewportCompassConfig::default();
        Self {
            region: InputRegionId::from_static("native.editor.viewport-compass"),
            rect: EditorRect::default(),
            host: graphics.create_ui_host(
                build_viewport_compass_surface(palette, ViewportCompassState::default(), config),
                [0, 0, 0, 0],
            ),
            config,
            active: false,
            last_sync: None,
        }
    }

    pub fn owner(&self) -> InputOwner {
        InputOwner::RetainedUi(self.region)
    }

    pub fn set_config(&mut self, config: ViewportCompassConfig) {
        let config = config.normalized();
        if self.config != config {
            self.config = config;
            self.last_sync = None;
        }
    }

    pub fn config(&self) -> ViewportCompassConfig {
        self.config
    }

    pub fn sync(
        &mut self,
        palette: StudioUiPalette,
        viewport: EditorRect,
        state: ViewportCompassState,
        enabled: bool,
    ) {
        let rect = compass_rect_for_viewport(viewport, self.config);
        let active = enabled && state.visible && rect.is_some();
        let rect = rect.unwrap_or_default();
        let render_config = if active {
            let mut config = self.config;
            config.size = rect.width;
            config.normalized()
        } else {
            self.config
        };
        let key = (palette, viewport, state, render_config, active);
        self.rect = rect;
        self.active = active;
        if self.last_sync == Some(key) {
            return;
        }
        self.last_sync = Some(key);
        if active {
            self.host.set_surface(build_viewport_compass_surface(
                palette,
                state,
                render_config,
            ));
        } else {
            self.host.set_surface(empty_compass_surface(palette));
            self.host.session_mut().interaction.focus.clear_focus();
        }
    }

    pub fn set_environment(&mut self, environment: raf_ui::UiEnvironment) {
        self.host.set_environment(environment);
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn has_interactive_hover(&self) -> bool {
        self.active && self.host.has_interactive_hover()
    }

    pub fn has_active_motion(&self) -> bool {
        self.active && self.host.has_active_motion()
    }

    pub fn has_active_text_repeat(&self) -> bool {
        self.active && self.host.has_active_text_repeat()
    }

    pub fn captures_keyboard_input(&self) -> bool {
        self.active && self.host.captures_keyboard_input()
    }

    pub fn cursor_hint(&self) -> raf_ui::UiCursorIcon {
        self.host.cursor_hint()
    }

    pub fn clear_focus(&mut self, router: &mut InputRouter) {
        self.host.session_mut().interaction.focus.clear_focus();
        router.cancel_owner(self.owner());
    }

    pub fn process_input<F>(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
        resolve: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        if !self.active {
            return Vec::new();
        }
        self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            resolve,
            input,
            router,
            self.owner(),
            UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        )
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

fn empty_compass_surface(palette: StudioUiPalette) -> UiSurface {
    UiSurface::new(
        COMPASS_SURFACE_ID,
        palette,
        UiNode::new(COMPASS_ROOT_ID, UiNodeKind::Root)
            .with_layout(UiLayout::fill(UiFlow::None))
            .with_style(UiStyle::transparent()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compass_rect_stays_at_the_viewport_lower_left() {
        let viewport = EditorRect::new(356.0, 46.0, 804.0, 620.0);
        let rect = compass_rect_for_viewport(viewport, ViewportCompassConfig::default())
            .expect("normal viewport should fit the compass");

        assert_eq!(rect.x, viewport.x + COMPASS_DEFAULT_MARGIN);
        assert_eq!(
            rect.y + rect.height,
            viewport.y + viewport.height - COMPASS_DEFAULT_MARGIN
        );
        assert_eq!(rect.width, rect.height);
    }

    #[test]
    fn projected_axis_positions_follow_orbit_rotation() {
        let config = ViewportCompassConfig::default();
        let first = ViewportCompassLayout::new(ViewportCompassState::from_orbit(0.0, 0.35), config);
        let second = ViewportCompassLayout::new(
            ViewportCompassState::from_orbit(std::f32::consts::FRAC_PI_2, 0.35),
            config,
        );

        assert_ne!(
            first.axis_points[0].position,
            second.axis_points[0].position
        );
        assert_ne!(
            first.axis_points[2].position,
            second.axis_points[2].position
        );
    }

    #[test]
    fn compass_commands_have_stable_camera_targets() {
        assert_eq!(
            ViewportCompassTarget::from_command("viewport.compass.snap:x"),
            Some(ViewportCompassTarget::X)
        );
        assert_eq!(
            ViewportCompassTarget::from_command("viewport.compass.snap:y"),
            Some(ViewportCompassTarget::Y)
        );
        assert_eq!(
            ViewportCompassTarget::from_command("viewport.compass.snap:z"),
            Some(ViewportCompassTarget::Z)
        );
        assert_eq!(
            ViewportCompassTarget::from_command("viewport.compass.reset"),
            Some(ViewportCompassTarget::Isometric)
        );
        assert!(ViewportCompassTarget::from_command("viewport.compass.snap:q").is_none());
    }

    #[test]
    fn default_axis_buttons_fit_inside_the_opaque_compass_panel() {
        let config = ViewportCompassConfig::default().normalized();
        let layout = ViewportCompassLayout::new(ViewportCompassState::default(), config);
        let endpoint_radius = compass_endpoint_size(layout.size, config.hit_radius) * 0.5;

        for endpoint in layout.axis_points {
            assert!(endpoint.position[0] - endpoint_radius >= 0.0);
            assert!(endpoint.position[1] - endpoint_radius >= 0.0);
            assert!(endpoint.position[0] + endpoint_radius <= layout.size);
            assert!(endpoint.position[1] + endpoint_radius <= layout.size);
        }
    }

    #[test]
    fn compass_surface_uses_a_translucent_panel_that_flips_with_the_theme() {
        let dark = build_viewport_compass_surface(
            StudioUiPalette::IndustrialDark,
            ViewportCompassState::default(),
            ViewportCompassConfig::default(),
        );
        let dark_background = dark
            .root
            .children
            .iter()
            .find(|node| node.id == COMPASS_BACKGROUND_ID)
            .expect("dark compass background");
        let dark_center = dark
            .root
            .children
            .iter()
            .find(|node| node.id == COMPASS_CENTER_ID)
            .expect("dark compass center");

        assert!(
            (dark_background.style.opacity - 0.1).abs() < 0.01,
            "panel style opacity should be 0.1 so the viewport reads through"
        );
        assert!(
            dark_background.style.fill[3] < 255,
            "panel should be translucent so the viewport shows through"
        );
        assert!(
            dark_background.style.fill[0] > 200
                && dark_background.style.fill[1] > 200
                && dark_background.style.fill[2] > 200,
            "dark theme should still use a light card so the widget reads on dark viewports"
        );
        assert!(
            dark_center.style.fill[3] == 0,
            "iso button should be outline-only so the panel below stays visible"
        );
        assert_eq!(dark_center.text_value.as_deref(), Some("ISO"));
    }

    #[test]
    fn compass_surface_keeps_the_classic_axis_labels_and_colors() {
        let surface = build_viewport_compass_surface(
            StudioUiPalette::IndustrialDark,
            ViewportCompassState::default(),
            ViewportCompassConfig::default(),
        );
        let expected = [
            ("x", "X", [220, 70, 70, 255]),
            ("y", "Y", [70, 220, 70, 255]),
            ("z", "Z", [70, 100, 220, 255]),
        ];

        for (slug, label, color) in expected {
            let button = surface
                .root
                .children
                .iter()
                .find(|node| node.id == format!("viewport.compass.axis.{slug}"))
                .expect("classic axis button");
            let text = surface
                .root
                .children
                .iter()
                .find(|node| node.id == format!("viewport.compass.axis.{slug}.label"))
                .expect("classic axis label");

            assert_eq!(button.style.fill, color);
            assert_eq!(text.text_value.as_deref(), Some(label));
            assert_eq!(
                text.text_style.as_ref().map(|style| style.color),
                Some(color)
            );
        }
    }
}
