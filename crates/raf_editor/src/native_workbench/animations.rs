//! Retained animation dock and isolated viewport pose preview.

use super::*;
use crate::commands::game::animation::preview;
use raf_assets::animation_asset;
use raf_core::scene::animation::{
    resolve_path, AnimationChannel, AnimationKey, AnimationLayer, AnimationRig, AnimationTrack,
};
use raf_core::scene::SceneNode;
use raf_ui::{UiAlign, UiOverflow, UiRange, UiSizeMode, UiSpacing, UiTextOverflow, UiTextStyle};

const TAB: &str = "animations";
const PLAYHEAD: &str = "animations.time";

#[derive(Default)]
pub(super) struct AnimationPanel {
    asset: Option<OpenAsset>,
    time: f32,
    layer: usize,
    preview: Option<(u64, uuid::Uuid, SceneGraph)>,
    status: Option<(&'static str, Option<String>)>,
}

struct OpenAsset {
    path: String,
    rig: AnimationRig,
}

fn selection<'a>(
    scene: &'a SceneGraph,
    selected: &[SceneNodeId],
) -> Option<(SceneNodeId, &'a SceneNode)> {
    let [id] = selected else { return None };
    scene
        .get(*id)
        .filter(|node| !node.name.is_empty())
        .map(|node| (*id, node))
}

impl NativeGameWorkbench {
    pub(super) fn open_animation_asset(&mut self, row: &str) {
        self.assets_surface.close_menus();
        self.assets_surface.close_modals();
        self.assets_surface.selected_asset = Some(row.to_owned());
        self.bottom_dock.select(TAB);
        self.animations = AnimationPanel::default();
        let result = self
            .assets_project_root
            .as_deref()
            .ok_or_else(|| "Project not found".to_string())
            .and_then(|root| animation_asset::load(root, row));
        match result {
            Ok(rig) => {
                self.animations.asset = Some(OpenAsset {
                    path: animation_asset::reference(row),
                    rig,
                })
            }
            Err(error) => self.animations.error(error),
        }
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    pub(crate) fn animation_preview_scene<'a>(
        &'a self,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
    ) -> Option<&'a SceneGraph> {
        if self.bottom_dock.is_collapsed() || !self.bottom_dock.has_active_tab(TAB) {
            return None;
        }
        let (_, node) = selection(scene, selected)?;
        self.animations
            .preview
            .as_ref()
            .filter(|(revision, owner, _)| {
                *revision == scene.document_revision() && *owner == node.uuid
            })
            .map(|(_, _, preview)| preview)
    }

    pub(super) fn handle_animation_action(
        &mut self,
        action: &UiAction,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        let name = match action {
            UiAction::SetRange { key, value } if key == PLAYHEAD => {
                if value.is_finite() && *value != self.animations.time {
                    self.animations.time = value.clamp(0.0, self.animations.duration());
                    self.animations.update_preview(scene, selected);
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    intents.push(NativeWorkbenchIntent::Command("animations.redraw".into()));
                }
                return true;
            }
            UiAction::Command { name } if name.starts_with("animations.") => name.as_str(),
            _ => return false,
        };
        let selected_node = selection(scene, selected);
        let project = self.assets_project_root.clone();
        match name {
            "animations.open" => {
                self.open_menu = None;
                self.open_submenu = None;
                self.assets_surface.close_menus();
                self.bottom_dock.select(TAB);
            }
            "animations.library" => {
                self.bottom_dock.select("assets");
                self.assets_surface.filter = AssetFilter::Animations;
                self.assets_surface.query.clear();
                self.assets_scroll_offset = 0.0;
            }
            "animations.open-linked" => {
                if let Some(path) =
                    selected_node.and_then(|(_, node)| node.animation_asset.as_deref())
                {
                    self.open_animation_asset(path);
                }
            }
            "animations.create" => {
                if let Some((_, node)) = selected_node {
                    if let Some(project) = project.as_deref() {
                        let rig = node
                            .animation
                            .clone()
                            .unwrap_or_else(|| rig_from_pose(node));
                        match animation_asset::create(project, &node.name, &rig) {
                            Ok(path) => {
                                self.project_catalog.refresh();
                                self.open_animation_asset(&path);
                                self.animations.status = Some(("animation.created", None));
                            }
                            Err(error) => self.animations.error(error),
                        }
                    }
                }
            }
            "animations.next-layer" => {
                if let Some(asset) = &self.animations.asset {
                    if !asset.rig.layers.is_empty() {
                        self.animations.layer =
                            (self.animations.layer + 1) % asset.rig.layers.len();
                    }
                }
            }
            "animations.preview" => self.animations.update_preview(scene, selected),
            "animations.reset-preview" => self.animations.preview = None,
            "animations.capture" => {
                if let (Some(project), Some((id, _))) = (project.as_deref(), selected_node) {
                    match self.animations.capture(project, scene, id) {
                        Ok(()) => self.animations.status = Some(("animation.key_saved", None)),
                        Err(error) => self.animations.error(error),
                    }
                }
            }
            "animations.link" | "animations.unlink" => {
                let asset = if name == "animations.link" {
                    self.animations
                        .asset
                        .as_ref()
                        .map(|asset| asset.path.as_str())
                } else {
                    None
                };
                if let (Some(project), Some((id, node))) = (project.as_deref(), selected_node) {
                    if name == "animations.unlink" || asset.is_some() {
                        match crate::commands::game::animation::validate_link(
                            scene, id, project, asset,
                        ) {
                            Ok(()) => {
                                let data = serde_json::json!({"target":node.uuid,"asset":asset});
                                intents.push(NativeWorkbenchIntent::Command(format!(
                                    "animations.bind:{data}"
                                )));
                                self.animations.status = Some((
                                    if asset.is_some() {
                                        "animation.linked"
                                    } else {
                                        "animation.unlinked"
                                    },
                                    None,
                                ));
                                self.animations.preview = None;
                            }
                            Err(error) => self.animations.error(error),
                        }
                    }
                }
            }
            _ => return false,
        }
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        intents.push(NativeWorkbenchIntent::Command("animations.redraw".into()));
        true
    }
}

impl AnimationPanel {
    fn error(&mut self, detail: String) {
        self.status = Some(("animation.error", Some(detail)));
    }

    fn duration(&self) -> f32 {
        self.asset
            .as_ref()
            .and_then(|asset| {
                asset
                    .rig
                    .layers
                    .iter()
                    .map(|layer| layer.start + layer.duration)
                    .reduce(f32::max)
            })
            .unwrap_or(1.0)
    }

    fn update_preview(&mut self, scene: &SceneGraph, selected: &[SceneNodeId]) {
        self.preview = None;
        if let (Some(asset), Some((id, node))) = (&self.asset, selection(scene, selected)) {
            match preview(scene, id, &asset.rig, f64::from(self.time)) {
                Ok(preview) => {
                    self.preview = Some((scene.document_revision(), node.uuid, preview));
                    self.status = None;
                }
                Err(error) => self.error(error),
            }
        }
    }

    fn capture(
        &mut self,
        project: &std::path::Path,
        scene: &SceneGraph,
        id: SceneNodeId,
    ) -> Result<(), String> {
        let asset = self.asset.as_mut().ok_or("Open an animation asset")?;
        let mut rig = asset.rig.clone();
        let layer = rig
            .layers
            .get_mut(self.layer)
            .ok_or("Animation has no editable layer")?;
        if layer.additive {
            return Err("Pose capture requires a non-additive layer".into());
        }
        let time = (self.time - layer.start).clamp(0.0, layer.duration);
        for track in &mut layer.tracks {
            let target = resolve_path(scene, id, &track.path)?;
            let node = scene.get(target).ok_or("Animation target was removed")?;
            let value = node_value(node, track.channel);
            let index = track.keys.partition_point(|key| key.time < time);
            let key = AnimationKey { time, value };
            if track
                .keys
                .get(index)
                .is_some_and(|entry| entry.time == time)
            {
                track.keys[index] = key;
            } else {
                track.keys.insert(index, key);
            }
        }
        animation_asset::save(project, &asset.path, &asset.rig, &rig)?;
        asset.rig = rig;
        self.preview = None;
        Ok(())
    }

    pub(super) fn surface(
        &self,
        palette: StudioUiPalette,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        width: f32,
    ) -> UiSurface {
        let tokens = palette.tokens();
        let target = selection(scene, selected).map(|(_, node)| node);
        let linked = target.and_then(|node| node.animation_asset.as_deref());
        let draft = self.asset.as_ref();
        let layer = draft.and_then(|asset| asset.rig.layers.get(self.layer));
        let mut controls = row("animations.controls");
        for (name, key, enabled) in [
            ("library", "animation.library", true),
            (
                "create",
                if target.is_some_and(|node| node.animation.is_some()) {
                    "animation.publish"
                } else {
                    "animation.create"
                },
                target.is_some(),
            ),
            ("open-linked", "animation.open_linked", linked.is_some()),
            (
                "link",
                "animation.link",
                target.is_some() && draft.is_some(),
            ),
            ("unlink", "animation.unlink", linked.is_some()),
        ] {
            controls = controls.with_child(button(name, key, enabled, palette));
        }
        let mut header = row("animations.active");
        if let Some(node) = target {
            header = header.with_child(label("animations.target", &node.name, palette));
        } else {
            header = header.with_child(
                UiNode::new("animations.target", UiNodeKind::Label)
                    .with_text_key("animation.select_target")
                    .with_layout(UiLayout::fit_content()),
            );
        }
        if let Some(asset) = draft {
            header = header.with_child(label("animations.asset", &asset.path, palette));
        } else {
            header = header.with_child(
                UiNode::new("animations.choose", UiNodeKind::Label)
                    .with_text_key("animation.choose_asset")
                    .with_layout(UiLayout::fit_content()),
            );
        }
        let mut root = UiNode::new("animations.root", UiNodeKind::Root)
            .with_layout(UiLayout {
                overflow: UiOverflow::ScrollY,
                padding: UiSpacing::xy(10.0, 4.0),
                gap: 4.0,
                ..UiLayout::fill(UiFlow::Column)
            })
            .with_style(UiStyle {
                fill: tokens.surface_alt,
                ..UiStyle::transparent()
            })
            .with_child(controls)
            .with_child(header);
        if let Some((key, detail)) = &self.status {
            let mut status = UiNode::new("animations.status", UiNodeKind::Label)
                .with_text_key(*key)
                .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill));
            status.tooltip_value = detail.clone();
            root = root.with_child(status);
        }
        if draft.is_some() {
            root = root.with_child(
                row("animations.timeline")
                    .with_child(button(
                        "preview",
                        "animation.preview",
                        target.is_some(),
                        palette,
                    ))
                    .with_child(
                        UiNode::range(
                            PLAYHEAD,
                            UiRange::new(PLAYHEAD, self.time, 0.0, self.duration(), 0.001),
                        )
                        .with_accessibility_label_key("animation.playhead")
                        .with_layout(UiLayout::fixed((width * 0.32).clamp(80.0, 380.0), 24.0)),
                    )
                    .with_child(label(
                        "animations.seconds",
                        &format!("{:.3} / {:.3} s", self.time, self.duration()),
                        palette,
                    ))
                    .with_child(button(
                        "reset-preview",
                        "animation.reset_preview",
                        self.preview.is_some(),
                        palette,
                    )),
            );
        }
        if let Some(layer) = layer {
            root = root.with_child(
                row("animations.tracks")
                    .with_child(button(
                        "next-layer",
                        "animation.next_layer",
                        draft.is_some_and(|asset| asset.rig.layers.len() > 1),
                        palette,
                    ))
                    .with_child(label("animations.layer", &layer.name, palette))
                    .with_child(button(
                        "capture",
                        "animation.capture",
                        target.is_some() && !layer.additive,
                        palette,
                    )),
            );
            for (index, track) in layer.tracks.iter().enumerate() {
                let name = track.path.join(" / ");
                let channel = match track.channel {
                    AnimationChannel::Position => "animation.position",
                    AnimationChannel::Rotation => "animation.rotation",
                    AnimationChannel::Scale => "animation.scale",
                };
                let times = track
                    .keys
                    .iter()
                    .take(8)
                    .map(|key| format!("{:.3}", key.time))
                    .collect::<Vec<_>>()
                    .join(" · ");
                root = root.with_child(
                    row(&format!("animations.track.{index}"))
                        .with_child(
                            UiNode::new(format!("animations.channel.{index}"), UiNodeKind::Label)
                                .with_text_key(channel)
                                .with_layout(UiLayout::fixed(80.0, 22.0)),
                        )
                        .with_child(label(&format!("animations.path.{index}"), &name, palette))
                        .with_child(label(
                            &format!("animations.keys.{index}"),
                            &format!("{}  |  {times}", track.keys.len()),
                            palette,
                        )),
                );
            }
        }
        UiSurface::new("animations", palette, root)
    }
}

fn node_value(node: &SceneNode, channel: AnimationChannel) -> glam::Vec3 {
    match channel {
        AnimationChannel::Position => node.position,
        AnimationChannel::Rotation => node.rotation,
        AnimationChannel::Scale => node.scale,
    }
}

fn rig_from_pose(node: &SceneNode) -> AnimationRig {
    let tracks = [
        AnimationChannel::Position,
        AnimationChannel::Rotation,
        AnimationChannel::Scale,
    ]
    .into_iter()
    .map(|channel| AnimationTrack {
        path: Vec::new(),
        channel,
        stepped: false,
        keys: vec![AnimationKey {
            time: 0.0,
            value: node_value(node, channel),
        }],
    })
    .collect();
    AnimationRig {
        impact: None,
        layers: vec![AnimationLayer {
            name: node.name.chars().take(32).collect(),
            duration: 1.0,
            looping: false,
            weight: 1.0,
            start: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            motion_speed: None,
            additive: false,
            tracks,
        }],
    }
}

fn row(id: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 8.0,
        overflow: UiOverflow::ScrollX,
        ..UiLayout::fixed(0.0, 25.0).with_width_mode(UiSizeMode::Fill)
    })
}

fn label(id: &str, text: &str, palette: StudioUiPalette) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_value(text)
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout::fit_content().with_text_safe_area(true))
}

fn button(name: &str, key: &'static str, enabled: bool, palette: StudioUiPalette) -> UiNode {
    let command = format!("animations.{name}");
    let tokens = palette.tokens();
    let mut button = UiNode::new(&command, UiNodeKind::Button)
        .with_text_key(key)
        .with_tooltip_key(key)
        .disabled(!enabled)
        .with_text_style(UiTextStyle::body(if enabled {
            tokens.text
        } else {
            tokens.text_muted
        }))
        .with_style(UiStyle {
            fill: tokens.surface,
            ..UiStyle::transparent()
        })
        .with_layout(UiLayout {
            padding: UiSpacing::xy(8.0, 2.0),
            ..UiLayout::fit_content().with_text_safe_area(true)
        });
    if enabled {
        button = button
            .focusable()
            .with_event(raf_ui::UiEventBinding::command(
                raf_ui::UiEventKind::Click,
                &command,
            ))
            .with_event(raf_ui::UiEventBinding::command(
                raf_ui::UiEventKind::KeyPress("enter".into()),
                &command,
            ));
    }
    button
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animations_dock_preview_uses_asset_keys_without_changing_the_document() {
        let mut scene = SceneGraph::new();
        let id = scene.add_root("Animated");
        let mut rig = rig_from_pose(scene.get(id).unwrap());
        rig.layers[0].tracks[0].keys.push(AnimationKey {
            time: 1.0,
            value: glam::Vec3::Y,
        });
        let mut panel = AnimationPanel {
            asset: Some(OpenAsset {
                path: "clip.animation.json".into(),
                rig,
            }),
            time: 0.5,
            ..AnimationPanel::default()
        };
        let revision = scene.document_revision();
        panel.update_preview(&scene, &[id]);
        let (previewed_revision, _, snapshot) = panel.preview.as_ref().unwrap();
        assert_eq!(*previewed_revision, revision);
        assert_eq!(snapshot.get(id).unwrap().position, glam::Vec3::Y * 0.5);
        assert_eq!(scene.get(id).unwrap().position, glam::Vec3::ZERO);
        let surface = panel.surface(StudioUiPalette::IndustrialDark, &scene, &[id], 640.0);
        assert!(surface
            .root
            .children
            .iter()
            .any(|node| node.id == "animations.timeline"));
        assert!(surface
            .root
            .children
            .iter()
            .any(|node| node.id == "animations.controls"));
    }
}
