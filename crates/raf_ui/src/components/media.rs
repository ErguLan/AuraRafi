//! Resource-keyed image composition with reserved geometry and explicit states.
use crate::*;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum UiImageViewShape {
    #[default]
    Plain,
    Rounded(f32),
}

#[derive(Debug, Clone)]
pub struct UiImageView {
    source: UiImageSource,
    fit: UiImageFit,
    options: UiImageOptions,
    shape: UiImageViewShape,
    tint: Option<[u8; 4]>,
}

impl UiImageView {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: UiImageSource::new(source),
            fit: UiImageFit::Contain,
            options: Default::default(),
            shape: Default::default(),
            tint: None,
        }
    }
    pub fn fit(mut self, fit: UiImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn tint(mut self, tint: [u8; 4]) -> Self {
        self.tint = Some(tint);
        self
    }
    pub fn shape(mut self, shape: UiImageViewShape) -> Self {
        self.shape = shape;
        self
    }
    pub fn options(mut self, options: UiImageOptions) -> Self {
        self.options = options;
        self
    }
    pub fn alt(mut self, key: impl Into<String>) -> Self {
        self.options.alt_text_key = Some(key.into());
        self
    }

    /// `layout.with_aspect_ratio(...)` reserves responsive height before the
    /// image finishes loading. The existing store owns pending/error pixels.
    pub fn build(self, id: impl Into<String>, layout: UiLayout) -> UiNode {
        let mut node = UiNode::new(id, UiNodeKind::Image).with_layout(layout);
        self.apply_to(&mut node);
        node.with_class("raf-image")
    }

    /// Preserves the caller's identity, layout, classes and event bindings.
    pub fn apply_to(self, node: &mut UiNode) {
        let radius = match self.shape {
            UiImageViewShape::Plain => 0.0,
            UiImageViewShape::Rounded(r) => r.max(0.0),
        };
        let alt = self.options.alt_text_key.clone();
        node.kind = UiNodeKind::Image;
        node.control = UiControl::Image(UiImage {
            source: self.source,
            fit: self.fit,
            tint: self.tint,
            options: self.options,
        });
        node.icon = None;
        node.visual.corners = Some([radius; 4]);
        node.visual.clip_children = true;
        if let Some(alt) = alt {
            node.accessibility_label_key = Some(alt);
        }
    }
}
