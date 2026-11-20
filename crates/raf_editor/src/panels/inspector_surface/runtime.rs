//! Runtime component sections: audio, physics, and camera lens controls.

use super::*;
use super::components::action_button;
use super::fields::{dropdown_field, range_labeled, section_label, text_field, toggle_row};
use super::transform::vector_row;

pub(super) fn audio_section(
    palette: StudioUiPalette,
    id: SceneNodeId,
    node: &raf_core::scene::SceneNode,
) -> UiNode {
    UiNode::new("inspector.audio", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(toggle_row(
            palette,
            "app.audio_enabled",
            "inspector.audio.enabled",
            node.audio_source.enabled,
        ))
        .with_child(text_field(
            "inspector.audio.clip",
            "inspector.audio.clip",
            &node.audio_source.clip,
            palette,
            1.0,
        ))
        .with_child(toggle_row(
            palette,
            "app.audio_autoplay",
            "inspector.audio.autoplay",
            node.audio_source.autoplay,
        ))
        .with_child(toggle_row(
            palette,
            "app.audio_looping",
            "inspector.audio.looping",
            node.audio_source.looping,
        ))
        .with_child(range_labeled(
            palette,
            "app.audio_volume",
            "inspector.audio.volume",
            node.audio_source.volume,
            0.0,
            1.0,
            0.01,
        ))
        .with_child(
            UiNode::new("inspector.audio.id", UiNodeKind::Label)
                .with_text_value(format!("Node {}", id.0))
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fit_content()),
        )
}

fn collider_options(current: raf_core::scene::ColliderType) -> (Vec<UiSelectOption>, usize) {
    let kinds = [
        raf_core::scene::ColliderType::None,
        raf_core::scene::ColliderType::Aabb,
        raf_core::scene::ColliderType::ConvexHull,
        raf_core::scene::ColliderType::MeshCollider,
    ];
    let selected_index = kinds.iter().position(|kind| *kind == current).unwrap_or(0);
    let options = kinds
        .into_iter()
        .map(|kind| {
            UiSelectOption::new(
                kind_label(kind).to_ascii_lowercase(),
                collider_label_key(kind),
            )
        })
        .collect();
    (options, selected_index)
}

fn body_type_options(current: raf_core::scene::RigidBodyType) -> (Vec<UiSelectOption>, usize) {
    let kinds = [
        raf_core::scene::RigidBodyType::Static,
        raf_core::scene::RigidBodyType::Dynamic,
        raf_core::scene::RigidBodyType::Kinematic,
    ];
    let selected_index = kinds.iter().position(|kind| *kind == current).unwrap_or(0);
    let options = kinds
        .into_iter()
        .map(|kind| {
            UiSelectOption::new(
                body_type_label(kind).to_ascii_lowercase(),
                body_type_label_key(kind),
            )
        })
        .collect();
    (options, selected_index)
}

pub(super) fn physics_section(
    palette: StudioUiPalette,
    _id: SceneNodeId,
    node: &raf_core::scene::SceneNode,
    open_dropdown: Option<InspectorDropdown>,
    display_unit: DisplayUnit,
) -> UiNode {
    let (collider_options, collider_selected_index) = collider_options(node.collider.collider_type);
    let (body_options, body_selected_index) = body_type_options(node.rigid_body.body_type);
    UiNode::new("inspector.physics", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(toggle_row(
            palette,
            "app.enable_rigidbody",
            "inspector.physics.enabled",
            node.rigid_body.enabled,
        ))
        .with_child(toggle_row(
            palette,
            "app.use_gravity",
            "inspector.physics.gravity",
            node.rigid_body.use_gravity,
        ))
        .with_child(toggle_row(
            palette,
            "runtime.physics_character",
            "inspector.physics.character",
            node.rigid_body.is_character,
        ))
        .with_child(toggle_row(
            palette,
            "app.is_trigger",
            "inspector.physics.trigger",
            node.rigid_body.is_trigger,
        ))
        .with_child(dropdown_field(
            palette,
            "inspector.collider",
            "app.collider_type",
            InspectorDropdown::Collider,
            open_dropdown,
            collider_options,
            collider_selected_index,
        ))
        .with_child(dropdown_field(
            palette,
            "inspector.body-type",
            "app.body_type",
            InspectorDropdown::BodyType,
            open_dropdown,
            body_options,
            body_selected_index,
        ))
        .with_child(physics_text_row(palette, "mass", "runtime.physics_mass"))
        .with_child(physics_text_row(palette, "friction", "runtime.physics_friction"))
        .with_child(physics_text_row(palette, "restitution", "runtime.physics_restitution"))
        .with_child(physics_text_row(palette, "angular_damping", "runtime.physics_angular_damping"))
        .with_child(physics_text_row(palette, "gravity_scale", "runtime.physics_gravity_scale"))
        .with_child(toggle_row(palette, "runtime.lock_translation_x", "inspector.physics.translation_lock_x", node.rigid_body.translation_locks[0]))
        .with_child(toggle_row(palette, "runtime.lock_translation_y", "inspector.physics.translation_lock_y", node.rigid_body.translation_locks[1]))
        .with_child(toggle_row(palette, "runtime.lock_translation_z", "inspector.physics.translation_lock_z", node.rigid_body.translation_locks[2]))
        .with_child(toggle_row(palette, "runtime.lock_rotation_x", "inspector.physics.rotation_lock_x", node.rigid_body.rotation_locks[0]))
        .with_child(toggle_row(palette, "runtime.lock_rotation_y", "inspector.physics.rotation_lock_y", node.rigid_body.rotation_locks[1]))
        .with_child(toggle_row(palette, "runtime.lock_rotation_z", "inspector.physics.rotation_lock_z", node.rigid_body.rotation_locks[2]))
        .with_child(physics_text_row(palette, "collision_layer", "runtime.collision_layer"))
        .with_child(physics_text_row(palette, "collision_mask", "runtime.collision_mask"))
        .with_child(toggle_row(
            palette,
            "runtime.physics_use_parent_body",
            "inspector.physics.body_owner",
            node.collider.body_owner.is_some(),
        ))
        .with_child(physics_joints_row(palette))
        .with_child(range_labeled(
            palette,
            "app.physics_damping",
            "inspector.physics.damping",
            node.rigid_body.damping,
            0.0,
            1.0,
            0.01,
        ))
        .with_child(vector_row(
            palette,
            "velocity",
            node.rigid_body.velocity,
            display_unit,
        ))
        .with_child(vector_row(
            palette,
            "angular_velocity",
            node.rigid_body.angular_velocity,
            display_unit,
        ))
}

fn physics_joints_row(palette: StudioUiPalette) -> UiNode {
    let key = "inspector.physics.joints.text";
    UiNode::new("inspector.physics.joints.row", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, "runtime.physics_joints"))
        .with_child(section_label(palette, "runtime.physics_joint_select_hint"))
        .with_child(
            UiNode::new("inspector.physics.joints.buttons", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::RowWrap,
                    gap: 4.0,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(action_button(palette, "inspector.joint.fixed", "runtime.joint_fixed", "inspector.physics.add-joint:fixed".into()))
                .with_child(action_button(palette, "inspector.joint.hinge", "runtime.joint_hinge", "inspector.physics.add-joint:hinge".into()))
                .with_child(action_button(palette, "inspector.joint.slider", "runtime.joint_slider", "inspector.physics.add-joint:slider".into()))
                .with_child(action_button(palette, "inspector.joint.ball", "runtime.joint_ball", "inspector.physics.add-joint:ball".into()))
                .with_child(action_button(palette, "inspector.joint.spring", "runtime.joint_spring", "inspector.physics.add-joint:spring".into())),
        )
        .with_child(
            UiNode::text_input(
                key,
                UiTextInput {
                    value_key: key.to_string(),
                    placeholder_key: Some("runtime.physics_joints_hint".into()),
                    max_length: 32768,
                    multiline: false,
                    password: false,
                    submit_command: Some("inspector.physics.commit:joints".into()),
                },
            )
            .with_class("inspector-input")
            .with_tooltip_key("runtime.physics_joints_hint")
            .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill))
            .focusable(),
        )
}

fn physics_text_row(palette: StudioUiPalette, field: &str, label: &str) -> UiNode {
    let key = format!("inspector.physics.{field}.text");
    UiNode::new(format!("{key}.row"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, label).with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content()
        }))
        .with_child(
            UiNode::text_input(
                &key,
                UiTextInput {
                    value_key: key.clone(),
                    placeholder_key: None,
                    max_length: 24,
                    multiline: false,
                    password: false,
                    submit_command: Some(format!("inspector.physics.commit:{field}")),
                },
            )
            .with_class("inspector-input")
            .with_layout(UiLayout::fixed(100.0, 28.0))
            .focusable(),
        )
}
pub(super) fn camera_section(palette: StudioUiPalette, node: &raf_core::scene::SceneNode) -> UiNode {
    let mut section = UiNode::new("inspector.camera", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, "camera.component"))
        .with_child(toggle_row(
            palette,
            "camera.enabled",
            "inspector.camera.enabled",
            node.game_camera.is_some(),
        ));
    if let Some(lens) = &node.game_camera {
        section = section.with_child(toggle_row(
            palette,
            "camera.orthographic",
            "inspector.camera.orthographic",
            lens.orthographic,
        ));
        for field in ["fov_degrees", "near", "far", "ortho_scale"] {
            let key = format!("inspector.camera.{field}.text");
            section = section.with_child(
                UiNode::new(format!("{key}.row"), UiNodeKind::Toolbar)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        gap: 6.0,
                        ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
                    })
                    .with_child(
                        section_label(palette, &format!("camera.{field}")).with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fit_content()
                        }),
                    )
                    .with_child(
                        UiNode::text_input(
                            &key,
                            UiTextInput {
                                value_key: key.clone(),
                                placeholder_key: None,
                                max_length: 24,
                                multiline: false,
                                password: false,
                                submit_command: Some(format!("inspector.camera.commit:{field}")),
                            },
                        )
                        .with_class("inspector-input")
                        .with_layout(UiLayout::fixed(100.0, 28.0))
                        .focusable(),
                    ),
            );
        }
        section = section.with_child(action_button(
            palette,
            "inspector.camera.startup",
            "camera.use_startup",
            "project-settings.runtime.camera.selected".into(),
        ));
    }
    section
}
fn kind_label(kind: raf_core::scene::ColliderType) -> &'static str {
    match kind {
        raf_core::scene::ColliderType::None => "None",
        raf_core::scene::ColliderType::Aabb => "Aabb",
        raf_core::scene::ColliderType::ConvexHull => "ConvexHull",
        raf_core::scene::ColliderType::MeshCollider => "MeshCollider",
    }
}

fn collider_label_key(kind: raf_core::scene::ColliderType) -> &'static str {
    match kind {
        raf_core::scene::ColliderType::None => "app.collider_none",
        raf_core::scene::ColliderType::Aabb => "app.collider_aabb",
        raf_core::scene::ColliderType::ConvexHull => "app.collider_convex_hull",
        raf_core::scene::ColliderType::MeshCollider => "app.collider_mesh",
    }
}

fn body_type_label(kind: raf_core::scene::RigidBodyType) -> &'static str {
    match kind {
        raf_core::scene::RigidBodyType::Static => "Static",
        raf_core::scene::RigidBodyType::Dynamic => "Dynamic",
        raf_core::scene::RigidBodyType::Kinematic => "Kinematic",
    }
}

fn body_type_label_key(kind: raf_core::scene::RigidBodyType) -> &'static str {
    match kind {
        raf_core::scene::RigidBodyType::Static => "app.body_static",
        raf_core::scene::RigidBodyType::Dynamic => "app.body_dynamic",
        raf_core::scene::RigidBodyType::Kinematic => "app.body_kinematic",
    }
}
