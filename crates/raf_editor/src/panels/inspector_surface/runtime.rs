//! Runtime component sections: audio, physics, and camera lens controls.

use super::components::action_button;
use super::fields::{dropdown_field, range_labeled, section_label, text_field, toggle_row};
use super::transform::vector_row;
use super::*;

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
        .with_child(audio_input(
            "inspector.audio.clip.text",
            "inspector.audio.clip.text",
            &node.audio_source.clip,
            "inspector.audio.commit:clip",
        ))
        .with_child(audio_input(
            "inspector.audio.bus.text",
            "inspector.audio.bus.text",
            node.audio_source.bus.slug(),
            "inspector.audio.commit:bus",
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
        .with_child(toggle_row(
            palette,
            "app.audio_spatial",
            "inspector.audio.spatial",
            node.audio_source.spatial,
        ))
        .with_child(audio_input(
            "inspector.audio.min_distance.text",
            "inspector.audio.min_distance.text",
            &format!("{:.2}", node.audio_source.min_distance),
            "inspector.audio.commit:min_distance",
        ))
        .with_child(audio_input(
            "inspector.audio.max_distance.text",
            "inspector.audio.max_distance.text",
            &format!("{:.2}", node.audio_source.max_distance),
            "inspector.audio.commit:max_distance",
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
        .with_child(action_button(
            palette,
            "inspector.audio.preview",
            "app.audio_preview",
            format!("assets.audio.preview:{}", node.audio_source.clip),
        ))
        .with_child(
            UiNode::new("inspector.audio.id", UiNodeKind::Label)
                .with_text_value(format!("Node {}", id.0))
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fit_content()),
        )
}

fn audio_input(id: &str, value_key: &str, value: &str, submit_command: &str) -> UiNode {
    UiNode::text_input(
        id,
        UiTextInput {
            value_key: value_key.to_string(),
            placeholder_key: None,
            max_length: 256,
            multiline: false,
            password: false,
            submit_command: Some(submit_command.to_string()),
        },
    )
    .with_class("inspector-input")
    .with_text_value(value.to_string())
    .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill))
    .focusable()
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
        .with_child(physics_text_row(
            palette,
            "friction",
            "runtime.physics_friction",
        ))
        .with_child(physics_text_row(
            palette,
            "restitution",
            "runtime.physics_restitution",
        ))
        .with_child(physics_text_row(
            palette,
            "angular_damping",
            "runtime.physics_angular_damping",
        ))
        .with_child(physics_text_row(
            palette,
            "gravity_scale",
            "runtime.physics_gravity_scale",
        ))
        .with_child(toggle_row(
            palette,
            "runtime.lock_translation_x",
            "inspector.physics.translation_lock_x",
            node.rigid_body.translation_locks[0],
        ))
        .with_child(toggle_row(
            palette,
            "runtime.lock_translation_y",
            "inspector.physics.translation_lock_y",
            node.rigid_body.translation_locks[1],
        ))
        .with_child(toggle_row(
            palette,
            "runtime.lock_translation_z",
            "inspector.physics.translation_lock_z",
            node.rigid_body.translation_locks[2],
        ))
        .with_child(toggle_row(
            palette,
            "runtime.lock_rotation_x",
            "inspector.physics.rotation_lock_x",
            node.rigid_body.rotation_locks[0],
        ))
        .with_child(toggle_row(
            palette,
            "runtime.lock_rotation_y",
            "inspector.physics.rotation_lock_y",
            node.rigid_body.rotation_locks[1],
        ))
        .with_child(toggle_row(
            palette,
            "runtime.lock_rotation_z",
            "inspector.physics.rotation_lock_z",
            node.rigid_body.rotation_locks[2],
        ))
        .with_child(physics_text_row(
            palette,
            "collision_layer",
            "runtime.collision_layer",
        ))
        .with_child(physics_text_row(
            palette,
            "collision_mask",
            "runtime.collision_mask",
        ))
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
                .with_child(action_button(
                    palette,
                    "inspector.joint.fixed",
                    "runtime.joint_fixed",
                    "inspector.physics.add-joint:fixed".into(),
                ))
                .with_child(action_button(
                    palette,
                    "inspector.joint.hinge",
                    "runtime.joint_hinge",
                    "inspector.physics.add-joint:hinge".into(),
                ))
                .with_child(action_button(
                    palette,
                    "inspector.joint.slider",
                    "runtime.joint_slider",
                    "inspector.physics.add-joint:slider".into(),
                ))
                .with_child(action_button(
                    palette,
                    "inspector.joint.ball",
                    "runtime.joint_ball",
                    "inspector.physics.add-joint:ball".into(),
                ))
                .with_child(action_button(
                    palette,
                    "inspector.joint.spring",
                    "runtime.joint_spring",
                    "inspector.physics.add-joint:spring".into(),
                )),
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
pub(super) fn particle_section(
    palette: StudioUiPalette,
    id: SceneNodeId,
    node: &raf_core::scene::SceneNode,
) -> UiNode {
    let Some(component) = node.particle_system.as_ref() else {
        return UiNode::new("inspector.particle.empty", UiNodeKind::Panel)
            .with_layout(UiLayout::fit_content());
    };
    let node_id = id.0;
    let button =
        |id: &str, text_key: &str, command: String| action_button(palette, id, text_key, command);
    let mut section = UiNode::new("inspector.particle", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, "app.particle_system"))
        .with_child(text_field(
            "inspector.particle.asset.text",
            "inspector.particle.asset.text",
            &component.asset,
            palette,
            1.0,
        ))
        .with_child(toggle_row(
            palette,
            "app.particle_play_on_start",
            "inspector.particle.play_on_start",
            component.play_on_start,
        ))
        .with_child(toggle_row(
            palette,
            "app.particle_looping",
            "inspector.particle.looping",
            component.looping,
        ))
        .with_child(toggle_row(
            palette,
            "app.particle_visible",
            "inspector.particle.visible",
            component.visible,
        ))
        .with_child(
            UiNode::new("inspector.particle.playback", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    compact: UiCompactMode::Wrap,
                    gap: 4.0,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(button(
                    "inspector.particle.play",
                    "app.particle_playback.playing",
                    format!("inspector.particle.play:{node_id}"),
                ))
                .with_child(button(
                    "inspector.particle.pause",
                    "app.particle_playback.paused",
                    format!("inspector.particle.pause:{node_id}"),
                ))
                .with_child(button(
                    "inspector.particle.freeze",
                    "app.particle_playback.frozen",
                    format!("inspector.particle.freeze:{node_id}"),
                ))
                .with_child(button(
                    "inspector.particle.stop",
                    "app.particle_playback.stopped",
                    format!("inspector.particle.stop:{node_id}"),
                )),
        )
        .with_child(range_labeled(
            palette,
            "app.particle_time_scale",
            "inspector.particle.time_scale",
            component.time_scale,
            0.0,
            4.0,
            0.05,
        ))
        .with_child(range_labeled(
            palette,
            "app.particle_rate_scale",
            "inspector.particle.rate_scale",
            component.rate_scale,
            0.0,
            4.0,
            0.05,
        ))
        .with_child(range_labeled(
            palette,
            "app.particle_max_particles",
            "inspector.particle.max_particles",
            component.max_particles as f32,
            0.0,
            250_000.0,
            1_000.0,
        ))
        .with_child(section_label(palette, "app.particle_quality"));
    let qualities = [
        (
            "potato",
            "app.particle_quality.potato",
            raf_core::scene::ParticleQuality::Potato,
        ),
        (
            "low",
            "app.particle_quality.low",
            raf_core::scene::ParticleQuality::Low,
        ),
        (
            "medium",
            "app.particle_quality.medium",
            raf_core::scene::ParticleQuality::Medium,
        ),
        (
            "high",
            "app.particle_quality.high",
            raf_core::scene::ParticleQuality::High,
        ),
        (
            "epic",
            "app.particle_quality.epic",
            raf_core::scene::ParticleQuality::Epic,
        ),
    ];
    let mut quality_row = UiNode::new("inspector.particle.quality", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Wrap,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
    for (slug, key, quality) in qualities {
        let class = if component.quality == quality {
            "inspector-particle-quality inspector-particle-quality-active"
        } else {
            "inspector-particle-quality"
        };
        quality_row = quality_row.with_child(
            UiNode::new(
                format!("inspector.particle.quality.{slug}"),
                UiNodeKind::Button,
            )
            .with_class(class)
            .with_layout(UiLayout {
                min_size: [60.0, 26.0],
                ..UiLayout::fixed(0.0, 26.0)
            })
            .with_text_key(key)
            .with_text_style(UiTextStyle::button(palette.tokens().text))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("inspector.particle.quality:{slug}:{node_id}"),
            )),
        );
    }
    section.with_child(quality_row)
}

pub(super) fn camera_section(
    palette: StudioUiPalette,
    node: &raf_core::scene::SceneNode,
) -> UiNode {
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
pub(super) fn light_section(palette: StudioUiPalette, node: &raf_core::scene::SceneNode) -> UiNode {
    let mut section = UiNode::new("inspector.light", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, "app.light_component"))
        .with_child(toggle_row(
            palette,
            "light.enabled",
            "app.light_enabled",
            node.light.is_some(),
        ));
    if let Some(light) = &node.light {
        section = section
            .with_child(action_button(
                palette,
                "inspector.light.point",
                "app.light_point",
                "inspector.light.kind:point".into(),
            ))
            .with_child(action_button(
                palette,
                "inspector.light.spot",
                "app.light_spot",
                "inspector.light.kind:spot".into(),
            ));
        for (field, value) in [
            ("intensity", light.intensity),
            ("range", light.range),
            ("color", 0.0),
        ] {
            let key = format!("inspector.light.{field}.text");
            let display = if field == "color" {
                format!(
                    "#{:02X}{:02X}{:02X}",
                    light.color[0], light.color[1], light.color[2]
                )
            } else {
                format!("{value:.2}")
            };
            section = section.with_child(
                UiNode::new(format!("{key}.row"), UiNodeKind::Toolbar)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        gap: 6.0,
                        ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
                    })
                    .with_child(
                        section_label(
                            palette,
                            match field {
                                "intensity" => "app.light_intensity",
                                "range" => "app.light_range",
                                _ => "app.light_color",
                            },
                        )
                        .with_layout(UiLayout {
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
                                submit_command: Some(format!("inspector.light.commit:{field}")),
                            },
                        )
                        .with_class("inspector-input")
                        .with_text_value(display)
                        .with_layout(UiLayout::fixed(100.0, 28.0))
                        .focusable(),
                    ),
            );
        }
    }
    section
}

pub(super) fn surface_section(
    palette: StudioUiPalette,
    id: SceneNodeId,
    node: &raf_core::scene::SceneNode,
) -> UiNode {
    UiNode::new("inspector.surface", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, "app.surface"))
        .with_child(toggle_row(
            palette,
            "commands.param.unlit",
            "inspector.surface.unlit",
            node.surface.unlit,
        ))
        .with_child(toggle_row(
            palette,
            "commands.param.double_sided",
            "inspector.surface.double_sided",
            node.surface.double_sided,
        ))
        .with_child(surface_tiling_row(palette, id, node.surface.texture_tiling))
        .with_child(surface_pivot_row(palette, id, node.pivot))
}

fn surface_tiling_row(palette: StudioUiPalette, id: SceneNodeId, tiling: [f32; 2]) -> UiNode {
    UiNode::new("inspector.surface.tiling", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, "commands.param.tiling"))
        .with_child(surface_tiling_axis(palette, id, 0, "U", tiling[0]))
        .with_child(surface_tiling_axis(palette, id, 1, "V", tiling[1]))
}

fn surface_tiling_axis(
    palette: StudioUiPalette,
    id: SceneNodeId,
    axis: usize,
    label: &str,
    value: f32,
) -> UiNode {
    let next = |delta: f32| ((value + delta) * 100.0).round().clamp(25.0, 12_800.0) / 100.0;
    let axis_key = if axis == 0 { "x" } else { "y" };
    let command = |value: f32| {
        format!(
            "inspector.range:inspector.surface.tiling.{axis_key}:{value}:{}",
            id.0
        )
    };
    UiNode::new(
        format!("inspector.surface.tiling.{axis_key}"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 4.0,
        ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!("inspector.surface.tiling.{axis_key}.label"),
            UiNodeKind::Label,
        )
        .with_text_value(label)
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout::fixed(18.0, 24.0)),
    )
    .with_child(surface_step_button(
        palette,
        format!("inspector.surface.tiling.{axis_key}.minus"),
        "-",
        command(next(-0.25)),
    ))
    .with_child(
        UiNode::new(
            format!("inspector.surface.tiling.{axis_key}.value"),
            UiNodeKind::Label,
        )
        .with_text_value(format!("{value:.2}"))
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [48.0, 24.0],
            ..UiLayout::fixed(0.0, 24.0)
        }),
    )
    .with_child(surface_step_button(
        palette,
        format!("inspector.surface.tiling.{axis_key}.plus"),
        "+",
        command(next(0.25)),
    ))
}

fn surface_step_button(
    palette: StudioUiPalette,
    id: String,
    label: &str,
    command: String,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("inspector-button")
        .with_layout(UiLayout::fixed(26.0, 24.0))
        .with_text_value(label)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn surface_pivot_row(
    palette: StudioUiPalette,
    id: SceneNodeId,
    pivot: raf_core::scene::PivotMode,
) -> UiNode {
    let mut buttons = UiNode::new("inspector.surface.pivot.buttons", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
        });
    for (mode, label_key) in [
        (raf_core::scene::PivotMode::Center, "app.pivot_center"),
        (raf_core::scene::PivotMode::Base, "app.pivot_base"),
        (raf_core::scene::PivotMode::Corner, "app.pivot_corner"),
    ] {
        let active = mode == pivot;
        buttons = buttons.with_child(
            UiNode::new(
                format!("inspector.surface.pivot.{}", mode.slug()),
                UiNodeKind::Button,
            )
            .with_class("inspector-button")
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [0.0, 24.0],
                ..UiLayout::fixed(0.0, 24.0)
            })
            .with_text_key(label_key)
            .with_text_style(UiTextStyle::button(if active {
                palette.tokens().text
            } else {
                palette.tokens().text_muted
            }))
            .with_accessibility_selected(active)
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("inspector.pivot:{}:{}", mode.slug(), id.0),
            )),
        );
    }
    UiNode::new("inspector.surface.pivot", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, "app.pivot"))
        .with_child(buttons)
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
