//! Thin Rhai adapter for the shared, client-scoped UI API.
use super::rhai_backend::{with_ctx, RhaiResult};
use raf_ui::game_ui::UiElementRef;
use rhai::{Engine, ImmutableString};

pub(super) fn register(engine: &mut Engine) {
    engine.register_type_with_name::<UiElementRef>("UiElement");
    engine.register_fn(
        "ui_create",
        |instance: ImmutableString,
         parent: ImmutableString,
         id: ImmutableString,
         kind: ImmutableString|
         -> RhaiResult<UiElementRef> {
            with_ctx(|ctx| ctx.ui_create(&instance, &parent, &id, &kind))
        },
    );
    engine.register_fn("ui_remove", |element: UiElementRef| -> RhaiResult<()> {
        with_ctx(|ctx| ctx.ui_remove(&element))
    });
    engine.register_fn(
        "ui_rect",
        |element: UiElementRef,
         x: rhai::FLOAT,
         y: rhai::FLOAT,
         width: rhai::FLOAT,
         height: rhai::FLOAT|
         -> RhaiResult<()> {
            with_ctx(|ctx| {
                ctx.ui_rect(
                    &element,
                    raf_ui::UiRect::new(x as f32, y as f32, width as f32, height as f32),
                )
            })
        },
    );
    engine.register_fn(
        "ui_present",
        |instance: ImmutableString,
         mode: ImmutableString,
         target: ImmutableString|
         -> RhaiResult<()> {
            with_ctx(|ctx| {
                let mut mount = ctx
                    .view
                    .game_ui
                    .instances()
                    .get(instance.as_str())
                    .ok_or_else(|| {
                        crate::ScriptError::InvalidArgument("ui.instance_missing".into())
                    })?
                    .mount
                    .clone();
                mount.kind = match mode.as_str() {
                    "screen" => crate::ui_api::UiPresentationKind::Screen,
                    "camera" => crate::ui_api::UiPresentationKind::CameraOverlay,
                    "plane" => crate::ui_api::UiPresentationKind::CameraPlane,
                    "surface" => crate::ui_api::UiPresentationKind::Surface,
                    "billboard" => crate::ui_api::UiPresentationKind::Billboard,
                    _ => {
                        return Err(crate::ScriptError::InvalidArgument(
                            "ui.invalid_presentation".into(),
                        ))
                    }
                };
                mount.target = target.into();
                ctx.ui_mount(&instance, mount)
            })
        },
    );
    for (function, field) in [("ui_offset", 0), ("ui_rotation", 1)] {
        engine.register_fn(
            function,
            move |instance: ImmutableString,
                  x: rhai::FLOAT,
                  y: rhai::FLOAT,
                  z: rhai::FLOAT|
                  -> RhaiResult<()> {
                with_ctx(|ctx| {
                    let mut mount = ctx
                        .view
                        .game_ui
                        .instances()
                        .get(instance.as_str())
                        .ok_or_else(|| {
                            crate::ScriptError::InvalidArgument("ui.instance_missing".into())
                        })?
                        .mount
                        .clone();
                    if field == 0 {
                        mount.offset = [x as f32, y as f32, z as f32];
                    } else {
                        mount.rotation_degrees = [x as f32, y as f32, z as f32];
                    }
                    ctx.ui_mount(&instance, mount)
                })
            },
        );
    }
    engine.register_fn(
        "ui_size",
        |instance: ImmutableString, width: rhai::FLOAT, height: rhai::FLOAT| -> RhaiResult<()> {
            with_ctx(|ctx| {
                let mut mount = ctx
                    .view
                    .game_ui
                    .instances()
                    .get(instance.as_str())
                    .ok_or_else(|| {
                        crate::ScriptError::InvalidArgument("ui.instance_missing".into())
                    })?
                    .mount
                    .clone();
                mount.size_meters = [width as f32, height as f32];
                ctx.ui_mount(&instance, mount)
            })
        },
    );
    engine.register_fn(
        "ui_element",
        |instance: ImmutableString, node: ImmutableString| -> RhaiResult<UiElementRef> {
            with_ctx(|ctx| ctx.ui_element(&instance, &node))
        },
    );
    engine.register_fn(
        "ui_text",
        |element: UiElementRef, value: ImmutableString| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.ui_text(&element, &value))
        },
    );
    engine.register_fn(
        "ui_image",
        |element: UiElementRef, value: ImmutableString| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.ui_image(&element, &value))
        },
    );
    engine.register_fn(
        "ui_visible",
        |element: UiElementRef, value: bool| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.ui_visible(&element, value))
        },
    );
    engine.register_fn(
        "ui_enabled",
        |element: UiElementRef, value: bool| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.ui_enabled(&element, value))
        },
    );
    engine.register_fn(
        "ui_show",
        |instance: ImmutableString, value: bool| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.ui_show(&instance, value))
        },
    );
    engine.register_fn(
        "ui_clone",
        |source: ImmutableString, instance: ImmutableString| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.ui_clone(&source, &instance))
        },
    );
    engine.register_fn(
        "ui_close",
        |instance: ImmutableString| -> RhaiResult<bool> { with_ctx(|ctx| ctx.ui_close(&instance)) },
    );
    engine.register_fn(
        "ui_value",
        |key: ImmutableString, value: ImmutableString| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.ui_value(&key, &value))
        },
    );
}
