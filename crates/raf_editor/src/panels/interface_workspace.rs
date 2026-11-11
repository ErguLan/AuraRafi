//! Transient authoring view. No presentation choice changes the saved document.
use raf_ui::UiRect;

pub const TOOLBAR_HEIGHT: f32 = 48.0;
pub const STATUS_HEIGHT: f32 = 28.0;
pub const CANVAS_MARGIN: f32 = 32.0;
pub const HANDLE_SIZE: f32 = 8.0;
pub const MIN_ELEMENT_SIZE: f32 = 8.0;
pub const PREVIEW_PREFIX: &str = "ui.designer.preview.";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InterfaceView {
    #[default]
    Scene,
    Live,
    Canvas,
}

#[derive(Debug, Clone)]
pub struct InterfaceWorkspace {
    pub view: InterfaceView,
    pub viewport: [f32; 2],
    pub zoom: Option<f32>,
    pub expanded: std::collections::BTreeSet<String>,
    pub collapsed_nodes: std::collections::BTreeSet<String>,
    pub snap: bool,
    pub insert_open: bool,
    pub selection_box: Option<UiRect>,
    pub parent_box: Option<UiRect>,
}
impl Default for InterfaceWorkspace {
    fn default() -> Self {
        Self {
            view: InterfaceView::Scene,
            viewport: [1.0; 2],
            zoom: None,
            expanded: ["content", "layout", "appearance"].map(String::from).into(),
            collapsed_nodes: Default::default(),
            snap: true,
            insert_open: false,
            selection_box: None,
            parent_box: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InterfaceGeometry {
    pub stage: UiRect,
    pub artboard: UiRect,
    pub logical_size: [f32; 2],
    pub scale: f32,
}
impl InterfaceWorkspace {
    pub fn active(&self) -> bool {
        self.view != InterfaceView::Scene
    }
    pub fn geometry(&self, size: [f32; 2], preview: [u32; 2]) -> InterfaceGeometry {
        let stage = UiRect::new(
            0.0,
            TOOLBAR_HEIGHT,
            size[0].max(1.0),
            (size[1] - TOOLBAR_HEIGHT - STATUS_HEIGHT).max(1.0),
        );
        let logical_size = if self.view == InterfaceView::Canvas {
            preview.map(|v| v.max(1) as f32)
        } else {
            [stage.width, stage.height]
        };
        let scale = if self.view == InterfaceView::Canvas {
            self.zoom.unwrap_or_else(|| {
                ((stage.width - CANVAS_MARGIN * 2.0).max(1.0) / logical_size[0])
                    .min((stage.height - CANVAS_MARGIN * 2.0).max(1.0) / logical_size[1])
                    .clamp(0.1, 2.0)
            })
        } else {
            1.0
        };
        let scaled = logical_size.map(|v| v * scale);
        InterfaceGeometry {
            stage,
            artboard: UiRect::new(
                ((stage.width - scaled[0]) * 0.5).max(0.0),
                ((stage.height - scaled[1]) * 0.5).max(0.0),
                scaled[0],
                scaled[1],
            ),
            logical_size,
            scale,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum InterfaceGesture {
    Move,
    Resize([i8; 2]),
}
pub struct InterfaceDrag {
    pub before: raf_ui::UiDocument,
    pub start: [f32; 2],
    pub gesture: InterfaceGesture,
    pub scale: f32,
    pub rect: UiRect,
    pub parent_size: [f32; 2],
}

pub fn color_hex(color: [u8; 4]) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        color[0], color[1], color[2], color[3]
    )
}
pub fn parse_color(value: &str) -> Result<[u8; 4], String> {
    let hex = value.trim().trim_start_matches('#');
    if hex.is_ascii() && matches!(hex.len(), 6 | 8) {
        let mut color = [255; 4];
        for (i, channel) in color.iter_mut().take(hex.len() / 2).enumerate() {
            *channel =
                u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| "ui.invalid_value")?;
        }
        return Ok(color);
    }
    let channels = value
        .split(',')
        .map(|c| c.trim().parse::<u8>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "ui.invalid_value")?;
    channels.try_into().map_err(|_| "ui.invalid_value".into())
}
