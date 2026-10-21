//! Thin language adapter. Service policy, queries and state live in the shared Host API.
use super::rhai_backend::{from_dynamic, to_dynamic, with_ctx, RhaiResult};
use super::rhai_scene_api::{array, vector};
use crate::{query_api::QueryFilter, services::CursorCapture, NodeHandle, ScriptError};
use rhai::{Array, Dynamic, Engine, ImmutableString, INT};

fn pair(value: [f32; 2]) -> Array {
    value
        .into_iter()
        .map(|n| Dynamic::from_float(n as f64))
        .collect()
}
fn mouse_index(button: INT) -> crate::ScriptResult<i32> {
    if (0..=4).contains(&button) {
        Ok(button as i32)
    } else {
        Err(ScriptError::InvalidArgument("input.invalid_button".into()))
    }
}
fn filter(ignore: NodeHandle, tag: ImmutableString, triggers: bool) -> QueryFilter {
    QueryFilter {
        ignore: (ignore != NodeHandle::invalid()).then_some(ignore),
        tag: (!tag.is_empty()).then(|| tag.to_string()),
        include_triggers: triggers,
    }
}
pub(super) fn register(engine: &mut Engine) {
    engine.register_fn(
        "runtime_field",
        |record: Dynamic, key: ImmutableString, fallback: Dynamic| -> RhaiResult<Dynamic> {
            if key.len() > 128 {
                return Err("runtime.invalid_field".into());
            }
            Ok(record
                .try_cast::<rhai::Map>()
                .and_then(|m| m.get(key.as_str()).cloned())
                .unwrap_or(fallback))
        },
    );
    engine.register_fn(
        "pointer_axis",
        |source: ImmutableString, axis: INT| -> RhaiResult<f64> {
            with_ctx(|c| {
                let pair = match source.as_str() {
                    "relative" => c.input.mouse_relative,
                    "delta" => c.input.mouse_delta,
                    "wheel" => c.input.mouse_wheel,
                    "position" => c.input.mouse_position.unwrap_or([0.0; 2]),
                    _ => {
                        return Err(ScriptError::InvalidArgument(
                            "input.invalid_pointer_source".into(),
                        ))
                    }
                };
                pair.get(axis as usize)
                    .map(|n| *n as f64)
                    .ok_or_else(|| ScriptError::InvalidArgument("input.invalid_axis".into()))
            })
        },
    );
    engine.register_fn(
        "mouse_button",
        |button: INT, phase: ImmutableString| -> RhaiResult<bool> {
            with_ctx(|c| {
                if !(0..=4).contains(&button) {
                    return Err(ScriptError::InvalidArgument("input.invalid_button".into()));
                }
                Ok(match phase.as_str() {
                    "held" => c.input.mouse_held.contains(&(button as i32)),
                    "pressed" => c.input.mouse_pressed.contains(&(button as i32)),
                    "released" => c.input.mouse_released.contains(&(button as i32)),
                    _ => return Err(ScriptError::InvalidArgument("input.invalid_phase".into())),
                })
            })
        },
    );
    engine.register_fn(
        "was_key_just_released",
        |key: ImmutableString| -> RhaiResult<bool> {
            with_ctx(|c| Ok(c.input.binding_released(&key)))
        },
    );
    engine.register_fn(
        "was_action_just_released",
        |key: ImmutableString| -> RhaiResult<bool> {
            with_ctx(|c| Ok(c.input.binding_released(&format!("action:{key}"))))
        },
    );
    engine.register_fn(
        "was_mouse_just_pressed",
        |button: INT| -> RhaiResult<bool> {
            with_ctx(|c| Ok(c.input.mouse_pressed.contains(&mouse_index(button)?)))
        },
    );
    engine.register_fn(
        "was_mouse_just_released",
        |button: INT| -> RhaiResult<bool> {
            with_ctx(|c| Ok(c.input.mouse_released.contains(&mouse_index(button)?)))
        },
    );
    engine.register_fn("mouse_position", || -> RhaiResult<Dynamic> {
        with_ctx(|c| {
            Ok(c.input
                .mouse_position
                .map(|v| Dynamic::from_array(pair(v)))
                .unwrap_or(Dynamic::UNIT))
        })
    });
    engine.register_fn("mouse_delta", || -> RhaiResult<Array> {
        with_ctx(|c| Ok(pair(c.input.mouse_delta)))
    });
    engine.register_fn("mouse_relative", || -> RhaiResult<Array> {
        with_ctx(|c| Ok(pair(c.input.mouse_relative)))
    });
    engine.register_fn("mouse_wheel", || -> RhaiResult<Array> {
        with_ctx(|c| Ok(pair(c.input.mouse_wheel)))
    });
    engine.register_fn("input_axis", |name: ImmutableString| -> RhaiResult<f64> {
        with_ctx(|c| {
            Ok(c.input
                .axes
                .get(&name.to_ascii_lowercase())
                .copied()
                .unwrap_or(0.0) as f64)
        })
    });
    engine.register_fn(
        "set_cursor",
        |mode: ImmutableString, visible: bool| -> RhaiResult<()> {
            with_ctx(|c| {
                let mode = match mode.as_str() {
                    "free" => CursorCapture::Free,
                    "locked" => CursorCapture::Locked,
                    "confined" => CursorCapture::Confined,
                    _ => {
                        return Err(ScriptError::InvalidArgument(
                            "input.invalid_cursor_mode".into(),
                        ))
                    }
                };
                c.set_cursor(mode, visible)
            })
        },
    );
    engine.register_fn("cursor_capture", || -> RhaiResult<String> {
        with_ctx(|c| Ok(c.view.services.effective_cursor.name().into()))
    });
    engine.register_fn(
        "raycast",
        |origin: Array, direction: Array, distance: f64| -> RhaiResult<Dynamic> {
            with_ctx(|c| {
                hit(c.raycast(
                    vector(origin)?,
                    vector(direction)?,
                    distance as f32,
                    &QueryFilter::default(),
                )?)
            })
        },
    );
    engine.register_fn(
        "raycast",
        |origin: Array,
         direction: Array,
         distance: f64,
         ignore: NodeHandle,
         tag: ImmutableString,
         triggers: bool|
         -> RhaiResult<Dynamic> {
            with_ctx(|c| {
                hit(c.raycast(
                    vector(origin)?,
                    vector(direction)?,
                    distance as f32,
                    &filter(ignore, tag, triggers),
                )?)
            })
        },
    );
    engine.register_fn(
        "overlap_aabb",
        |min: Array, max: Array| -> RhaiResult<Array> {
            with_ctx(|c| {
                Ok(
                    c.overlap_aabb(vector(min)?, vector(max)?, &QueryFilter::default())?
                        .into_iter()
                        .map(Dynamic::from)
                        .collect(),
                )
            })
        },
    );
    engine.register_fn(
        "overlap_aabb",
        |min: Array,
         max: Array,
         ignore: NodeHandle,
         tag: ImmutableString,
         triggers: bool|
         -> RhaiResult<Array> {
            with_ctx(|c| {
                Ok(
                    c.overlap_aabb(vector(min)?, vector(max)?, &filter(ignore, tag, triggers))?
                        .into_iter()
                        .map(Dynamic::from)
                        .collect(),
                )
            })
        },
    );
    engine.register_fn(
        "progress_set",
        |key: ImmutableString, value: Dynamic| -> RhaiResult<()> {
            with_ctx(|c| c.view.services.progress.set(&key, from_dynamic(value)?))
        },
    );
    engine.register_fn(
        "progress_get",
        |key: ImmutableString, fallback: Dynamic| -> RhaiResult<Dynamic> {
            with_ctx(|c| {
                Ok(to_dynamic(
                    c.view.services.progress.get(&key, from_dynamic(fallback)?),
                ))
            })
        },
    );
    engine.register_fn(
        "progress_remove",
        |key: ImmutableString| -> RhaiResult<()> {
            with_ctx(|c| c.view.services.progress.remove(&key))
        },
    );
    engine.register_fn("progress_clear", || -> RhaiResult<()> {
        with_ctx(|c| c.view.services.progress.clear())
    });
    engine.register_fn("progress_discard", || -> RhaiResult<()> {
        with_ctx(|c| c.view.services.progress.discard())
    });
    engine.register_fn(
        "progress_profile",
        |name: ImmutableString| -> RhaiResult<()> {
            with_ctx(|c| c.view.services.progress.set_profile(&name))
        },
    );
    engine.register_fn(
        "progress_save",
        |slot: ImmutableString| -> RhaiResult<INT> {
            with_ctx(|c| {
                c.view
                    .services
                    .progress
                    .request(&slot, false)
                    .map(|id| id as INT)
            })
        },
    );
    engine.register_fn(
        "progress_load",
        |slot: ImmutableString| -> RhaiResult<INT> {
            with_ctx(|c| {
                c.view
                    .services
                    .progress
                    .request(&slot, true)
                    .map(|id| id as INT)
            })
        },
    );
    engine.register_fn(
        "progress_load_backup",
        |slot: ImmutableString| -> RhaiResult<INT> {
            with_ctx(|c| {
                c.view
                    .services
                    .progress
                    .request_backup(&slot)
                    .map(|id| id as INT)
            })
        },
    );
    engine.register_fn("progress_apply_loaded", || -> RhaiResult<()> {
        with_ctx(|c| c.view.services.progress.apply_loaded())
    });
    engine.register_fn(
        "progress_loaded_get",
        |key: ImmutableString, fallback: Dynamic| -> RhaiResult<Dynamic> {
            with_ctx(|c| {
                Ok(to_dynamic(
                    c.view
                        .services
                        .progress
                        .loaded_get(&key, from_dynamic(fallback)?),
                ))
            })
        },
    );
    engine.register_fn("progress_status", || -> RhaiResult<Dynamic> {
        with_ctx(|c| Ok(to_dynamic(c.view.services.progress.report())))
    });
    engine.register_fn("request_session", |id: ImmutableString| -> RhaiResult<()> {
        with_ctx(|c| {
            c.request_session(
                uuid::Uuid::parse_str(&id).map_err(|_| {
                    ScriptError::InvalidArgument("runtime.invalid_session_uuid".into())
                })?,
            )
        })
    });
    engine.register_fn("session_status", || -> RhaiResult<Dynamic> {
        with_ctx(|c| Ok(to_dynamic(c.session_report())))
    });
}
fn hit(value: Option<crate::query_api::RayHit>) -> crate::ScriptResult<Dynamic> {
    let Some(hit) = value else {
        return Ok(Dynamic::UNIT);
    };
    let mut map = rhai::Map::new();
    map.insert("entity".into(), Dynamic::from(hit.entity));
    map.insert(
        "position".into(),
        Dynamic::from_array(array(hit.position.to_array())),
    );
    map.insert(
        "normal".into(),
        Dynamic::from_array(array(hit.normal.to_array())),
    );
    map.insert("distance".into(), Dynamic::from_float(hit.distance as f64));
    Ok(Dynamic::from_map(map))
}
