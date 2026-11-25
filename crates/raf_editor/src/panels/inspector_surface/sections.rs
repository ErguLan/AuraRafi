//! Inspector content composition and read-only metadata/debug sections.

use super::behavior::build_behavior_sections;
use super::components::{
    metadata_key_line, metadata_line, node_info, session_context_bar, sessions_content, status_row,
};
use super::fields::{
    color_picker, dropdown_field, primitive_options, range_labeled, section_label,
    texture_dropdown, toggle_row,
};
use super::runtime::{
    audio_section, camera_section, light_section, particle_section, physics_section,
    surface_section,
};
use super::transform::{transform_actions, vector_row};
use super::*;

fn metadata_section(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    id: SceneNodeId,
    node: &raf_core::scene::SceneNode,
) -> UiNode {
    UiNode::new("inspector.metadata-section", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(metadata_line(palette, "app.uuid", &node.uuid.to_string()))
        .with_child(metadata_line(
            palette,
            "app.entity_index",
            &node
                .entity_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ))
        .with_child(metadata_line(
            palette,
            "app.scene_path",
            &scene.node_path(id).unwrap_or_else(|| "-".to_string()),
        ))
        .with_child(metadata_line(
            palette,
            "app.source_asset",
            node.source_asset.as_deref().unwrap_or("-"),
        ))
        .with_child(metadata_line(
            palette,
            "app.schema_version",
            &node
                .source_schema_version
                .map(|version| version.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ))
}

fn debug_section(
    palette: StudioUiPalette,
    id: SceneNodeId,
    node: &raf_core::scene::SceneNode,
) -> UiNode {
    UiNode::new("inspector.debug-section", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(metadata_key_line(
            palette,
            "app.renderable",
            if node.is_folder || node.primitive == Primitive::Empty {
                "app.no"
            } else {
                "app.yes"
            },
        ))
        .with_child(metadata_line(palette, "app.node_id", &id.0.to_string()))
        .with_child(metadata_key_line(
            palette,
            "app.node_kind",
            if node.is_folder {
                "app.folder"
            } else {
                "app.entity"
            },
        ))
}
pub(super) fn section_title(
    palette: StudioUiPalette,
    key: &str,
    section: InspectorSection,
    expanded: bool,
) -> UiNode {
    raf_ui::components::disclosure_header(
        format!("inspector.section.{}", section.slug()),
        key,
        section_icon(key),
        expanded,
        format!("inspector.section.toggle:{}", section.slug()),
        palette,
    )
    .with_class("inspector-section-title")
}

fn section_icon(key: &str) -> UiIconId {
    match key {
        "app.identity" => UiIconId::Entity,
        "app.transform" => UiIconId::Move,
        "app.appearance" => UiIconId::Shaded,
        "app.components" => UiIconId::Node,
        "app.metadata" => UiIconId::Assets,
        "app.debug" => UiIconId::Settings,
        "app.primitive" => UiIconId::Cube,
        "app.material" => UiIconId::Shaded,
        "app.variables" => UiIconId::Node,
        "app.audio_source" => UiIconId::Assets,
        "app.physics" => UiIconId::Warning,
        _ => UiIconId::ChevronRight,
    }
}
pub(super) fn content(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    selected: Option<SceneNodeId>,
    sessions: &ProjectSessionRegistry,
    view: InspectorViewState,
    display_unit: DisplayUnit,
    project_assets: &[String],
) -> UiNode {
    if view.tab == InspectorTab::Sessions {
        return sessions_content(palette, sessions);
    }
    let tokens = palette.tokens();
    let Some(id) = selected.filter(|id| scene.is_valid_node(*id)) else {
        return UiNode::new("inspector.empty.wrap", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 6.0,
                grow: 1.0,
                ..UiLayout::fill(UiFlow::Column)
            })
            .with_child(session_context_bar(palette, sessions))
            .with_child(
                UiNode::new("inspector.empty", UiNodeKind::Panel)
                    .with_class("inspector-empty")
                    .with_layout(UiLayout {
                        flow: UiFlow::Column,
                        align_items: UiAlign::Center,
                        justify_content: raf_ui::UiJustify::Center,
                        gap: 6.0,
                        padding: UiSpacing::same(12.0),
                        grow: 1.0,
                        ..UiLayout::fill(UiFlow::Column)
                    })
            .with_child(
                UiNode::new("inspector.empty.icon", UiNodeKind::Label)
                    .with_icon(
                        UiIcon::new(UiIconId::Entity)
                            .with_size(raf_render::api_graphic_basic::ui_surface::UiIconSize::Panel)
                            .with_tint(tokens.text_muted),
                    )
                    .with_layout(UiLayout::fixed(28.0, 28.0)),
            )
            .with_child(
                UiNode::new("inspector.empty.label", UiNodeKind::Label)
                    .with_text_key("app.inspector_empty")
                    .with_text_style(UiTextStyle::button(tokens.text))
                    .with_layout(UiLayout::fit_content()),
            )
            .with_child(
                UiNode::new("inspector.empty.hint", UiNodeKind::Label)
                    .with_text_key("app.inspector_empty_hint")
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fit_content()),
            ));
    };
    let node = scene.get(id).expect("selected node was validated");
    let mut scroll =
        UiNode::scroll_view("inspector.scroll", UiScrollAxis::Vertical).with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            grow: 1.0,
            padding: UiSpacing {
                left: 2.0,
                right: 4.0,
                top: 2.0,
                bottom: 12.0,
            },
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });

    scroll = scroll.with_child(session_context_bar(palette, sessions));
    let assets = project_assets
        .iter()
        .map(|path| raf_assets::authoring_catalog::normalized_reference(path))
        .collect();
    let diagnostics = crate::panels::hierarchy_model::details::badge_index(scene, Some(&assets));
    if let Some(badges) = diagnostics.get(&id) {
        for (index, key) in badges.iter().flat_map(|b| b.diagnostics.iter()).enumerate() {
            scroll = scroll.with_child(
                UiNode::new(format!("inspector.diagnostic.{index}"), UiNodeKind::Label)
                    .with_icon(UiIcon::new(UiIconId::Warning))
                    .with_text_key(*key)
                    .with_layout(UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill))
                    .with_tooltip_key(*key),
            );
        }
    }

    scroll = scroll.with_child(section_title(
        palette,
        "app.identity",
        InspectorSection::Identity,
        view.identity,
    ));
    if view.identity {
        scroll = scroll.with_child(
            UiNode::text_input(
                "inspector.name",
                UiTextInput {
                    value_key: "inspector.name".to_string(),
                    placeholder_key: Some("app.name".to_string()),
                    max_length: 256,
                    multiline: false,
                    password: false,
                    submit_command: Some(format!("inspector.rename.commit:{}", id.0)),
                },
            )
            .with_class("inspector-input")
            .with_layout(UiLayout {
                min_size: [100.0, 26.0],
                ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
            }),
        );
        scroll = scroll
            .with_child(node_info(palette, scene, node))
            .with_child(status_row(palette, id, node.visible, node.locked));
    }

    scroll = scroll.with_child(section_title(
        palette,
        "app.transform",
        InspectorSection::Transform,
        view.transform,
    ));
    if view.transform {
        scroll = scroll
            .with_child(vector_row(palette, "position", node.position, display_unit))
            .with_child(vector_row(palette, "rotation", node.rotation, display_unit))
            .with_child(vector_row(palette, "scale", node.scale, display_unit))
            .with_child(if node.primitive != Primitive::Empty && !node.is_folder {
                vector_row(
                    palette,
                    "size",
                    node.primitive.dimensions(node.scale),
                    display_unit,
                )
            } else {
                let dimensions = raf_core::scene::geometry::subtree_geometry_bounds(scene, id)
                    .map(|(min, max)| max - min)
                    .unwrap_or(Vec3::ZERO);
                metadata_line(
                    palette,
                    "app.world_dimensions",
                    &format!(
                        "{:.3} × {:.3} × {:.3} m",
                        dimensions.x, dimensions.y, dimensions.z
                    ),
                )
            })
            .with_child(transform_actions(palette, id));
    }

    scroll = scroll.with_child(section_title(
        palette,
        "app.appearance",
        InspectorSection::Appearance,
        view.appearance,
    ));
    if view.appearance {
        let (primitive_options, primitive_selected_index) = primitive_options(node.primitive);
        scroll = scroll
            .with_child(dropdown_field(
                palette,
                "inspector.primitive",
                "app.primitive_type",
                InspectorDropdown::Primitive,
                view.dropdown,
                primitive_options,
                primitive_selected_index,
            ))
            .with_child(section_label(palette, "app.material"))
            .with_child(texture_dropdown(
                palette,
                node.base_color_texture.as_deref(),
                project_assets,
                view.dropdown,
            ))
            .with_child(color_picker(palette, node.color, view.color_picker))
            .with_child(range_labeled(
                palette,
                "app.opacity",
                "inspector.appearance.opacity",
                f32::from(node.color.a) / 255.0 * 100.0,
                0.0,
                100.0,
                1.0,
            ))
            .with_child(surface_section(palette, id, node))
            .with_child(toggle_row(
                palette,
                "lighting.cast_shadows",
                "inspector.shadows.casts",
                node.casts_shadows,
            ))
            .with_child(toggle_row(
                palette,
                "lighting.receive_shadows",
                "inspector.shadows.receives",
                node.receives_shadows,
            ));
    }

    scroll = scroll.with_child(section_title(
        palette,
        "app.components",
        InspectorSection::Components,
        view.components,
    ));
    if view.components {
        scroll = scroll.with_child(camera_section(palette, node));
        scroll = scroll.with_child(light_section(palette, node));
        if node.particle_system.is_some() {
            scroll = scroll.with_child(particle_section(palette, id, node));
        }
        for child in build_behavior_sections(palette, scene, id, node, project_assets, view) {
            scroll = scroll.with_child(child);
        }
        scroll = scroll.with_child(section_title(
            palette,
            "app.audio_source",
            InspectorSection::Audio,
            view.audio,
        ));
        if view.audio {
            scroll = scroll.with_child(audio_section(palette, id, node));
        }
        scroll = scroll.with_child(section_title(
            palette,
            "app.physics",
            InspectorSection::Physics,
            view.physics,
        ));
        if view.physics {
            scroll = scroll.with_child(physics_section(
                palette,
                id,
                node,
                view.dropdown,
                display_unit,
            ));
        }
    }

    scroll = scroll.with_child(section_title(
        palette,
        "app.metadata",
        InspectorSection::Metadata,
        view.metadata,
    ));
    if view.metadata {
        scroll = scroll.with_child(metadata_section(palette, scene, id, node));
    }
    scroll = scroll.with_child(section_title(
        palette,
        "app.debug",
        InspectorSection::Debug,
        view.debug,
    ));
    if view.debug {
        scroll = scroll.with_child(debug_section(palette, id, node));
    }

    UiNode::new("inspector.content", UiNodeKind::Panel)
        .with_class("inspector-content")
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(scroll)
}
