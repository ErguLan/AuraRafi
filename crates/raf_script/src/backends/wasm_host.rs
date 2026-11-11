//! Typed imports delegate to the existing Host API, never to scene internals.
use crate::{NodeHandle, ScriptContext, ScriptError, ScriptResult, ScriptValue};
use glam::Vec3;

/// Zero is unassigned. Guest tokens are table entries, NOT raw SceneNodeId values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestEntity(pub u32);

pub enum InputQuery {
    KeyPressed(String),
    KeyReleased(String),
    Axis(String),
    MouseHeld(i32),
    MousePressed(i32),
    MouseReleased(i32),
    MousePosition,
    MouseDelta,
    MouseRelative,
    MouseWheel,
    CursorCapture,
}
pub enum ProgressImport {
    Set(String, ScriptValue),
    Get(String, ScriptValue),
    LoadedGet(String, ScriptValue),
    Remove(String),
    Clear,
    Discard,
    Profile(String),
    Save(String),
    Load(String),
    LoadBackup(String),
    ApplyLoaded,
    Status,
}
pub struct GuestRayHit {
    pub entity: GuestEntity,
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub distance: f32,
}

pub enum HostOperation {
    /// Optional typed UI imports; executor adapters still enforce guest memory and fuel limits.
    GameUi(crate::ui_api::UiImport),
    Owner,
    Entity(String),
    EntityParameter(String),
    Parameter(String, ScriptValue),
    IsValid(GuestEntity),
    Parent(GuestEntity),
    Child(GuestEntity, String),
    Reparent {
        entity: GuestEntity,
        parent: GuestEntity,
        keep_world: bool,
    },
    WorldPosition(GuestEntity),
    SetWorldPosition(GuestEntity, [f32; 3]),
    SetLocalPosition(GuestEntity, [f32; 3]),
    SetLocalRotation(GuestEntity, [f32; 3]),
    ActiveCamera,
    ActivateCamera(GuestEntity),
    ClearCamera,
    HasCamera(GuestEntity),
    AddCamera(GuestEntity),
    RemoveCamera(GuestEntity),
    SetFov(GuestEntity, f32),
    LookAt(GuestEntity, [f32; 3]),
    Follow {
        camera: GuestEntity,
        target: GuestEntity,
        offset: [f32; 3],
        aim: [f32; 3],
        sharpness: f32,
        dt: f32,
    },
    KeyHeld(String),
    Input(InputQuery),
    SetCursor(crate::services::CursorCapture, bool),
    Progress(ProgressImport),
    RequestSession(uuid::Uuid),
    SessionStatus,
    Raycast {
        origin: [f32; 3],
        direction: [f32; 3],
        distance: f32,
        ignore: GuestEntity,
        tag: Option<String>,
        triggers: bool,
    },
    OverlapAabb {
        min: [f32; 3],
        max: [f32; 3],
        ignore: GuestEntity,
        tag: Option<String>,
        triggers: bool,
    },
    IsClient,
    DeltaTime,
    Elapsed,
    GetProperty(GuestEntity, String),
    SetProperty(GuestEntity, String, ScriptValue),
    SendEvent {
        target: GuestEntity,
        name: String,
        data: ScriptValue,
    },
}

pub enum HostReply {
    Value(ScriptValue),
    Entity(GuestEntity),
    Entities(Vec<GuestEntity>),
    RayHit(Option<GuestRayHit>),
}

pub trait GuestImports {
    fn check_budget(&self) -> ScriptResult<()>;
    fn call(&mut self, operation: HostOperation) -> ScriptResult<HostReply>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachmentIdentity {
    runtime: u64,
    owner: Option<uuid::Uuid>,
    source: Option<String>,
}

impl AttachmentIdentity {
    pub(crate) fn from_context(ctx: &ScriptContext<'_>) -> ScriptResult<Self> {
        if ctx
            .behavior_source
            .is_none_or(|source| source.is_empty() || source.len() > 512)
        {
            return Err(ScriptError::InvalidArgument(
                "WASM requires an explicit attachment source".into(),
            ));
        }
        let owner = ctx
            .owner()
            .map(|h| {
                h.resolve(ctx).and_then(|id| {
                    ctx.scene
                        .get(id)
                        .map(|n| n.uuid)
                        .ok_or(ScriptError::InvalidHandle(h.raw()))
                })
            })
            .transpose()?;
        if ctx.owner.is_some() && owner.is_none() {
            return Err(ScriptError::InvalidArgument(
                "WASM attachment owner is missing".into(),
            ));
        }
        Ok(Self {
            runtime: ctx.instance_id,
            owner,
            source: ctx.behavior_source.map(str::to_owned),
        })
    }
}

#[derive(Default)]
pub(crate) struct GuestHandles {
    entries: Vec<NodeHandle>,
}

impl GuestHandles {
    fn export_token(&mut self, handle: NodeHandle, max: usize) -> ScriptResult<GuestEntity> {
        match self.export(Some(handle), max)? {
            HostReply::Entity(token) => Ok(token),
            _ => Err(ScriptError::InvalidArgument(
                "WASM entity export mismatch".into(),
            )),
        }
    }
    fn export(&mut self, handle: Option<NodeHandle>, max: usize) -> ScriptResult<HostReply> {
        let Some(handle) = handle else {
            return Ok(HostReply::Entity(GuestEntity(0)));
        };
        let index = if let Some(index) = self.entries.iter().position(|h| *h == handle) {
            index
        } else {
            if self.entries.len() >= max {
                return Err(ScriptError::InvalidArgument(
                    "WASM entity handle budget exceeded".into(),
                ));
            }
            self.entries.push(handle);
            self.entries.len() - 1
        };
        Ok(HostReply::Entity(GuestEntity((index + 1) as u32)))
    }
    fn get(&self, token: GuestEntity) -> ScriptResult<NodeHandle> {
        token
            .0
            .checked_sub(1)
            .and_then(|i| self.entries.get(i as usize))
            .copied()
            .ok_or(ScriptError::InvalidHandle(token.0 as u64))
    }
}

pub(crate) struct HostBridge<'a, 'ctx> {
    pub ctx: &'a mut ScriptContext<'ctx>,
    pub handles: &'a mut GuestHandles,
    pub max_handles: usize,
}

impl GuestImports for HostBridge<'_, '_> {
    fn check_budget(&self) -> ScriptResult<()> {
        self.ctx.check_budget()
    }
    fn call(&mut self, operation: HostOperation) -> ScriptResult<HostReply> {
        self.check_budget()?;
        let ctx = &mut *self.ctx;
        let handles = &mut *self.handles;
        let none = HostReply::Value(ScriptValue::None);
        Ok(match operation {
            HostOperation::GameUi(operation)=>HostReply::Value(crate::ui_api::dispatch_import(ctx,operation)?),
            HostOperation::Input(query) => HostReply::Value(input_value(ctx, query)?),
            HostOperation::SetCursor(mode, visible) => {
                ctx.set_cursor(mode, visible)?;
                none
            }
            HostOperation::Progress(operation) => {
                HostReply::Value(progress_value(&mut ctx.view.services.progress, operation)?)
            }
            HostOperation::RequestSession(id) => {
                ctx.request_session(id)?;
                none
            }
            HostOperation::SessionStatus => HostReply::Value(ctx.session_report()),
            HostOperation::Raycast {
                origin,
                direction,
                distance,
                ignore,
                tag,
                triggers,
            } => {
                let filter = crate::query_api::QueryFilter {
                    ignore: if ignore.0 == 0 {
                        None
                    } else {
                        Some(handles.get(ignore)?)
                    },
                    tag,
                    include_triggers: triggers,
                };
                let hit = ctx.raycast(
                    Vec3::from_array(origin),
                    Vec3::from_array(direction),
                    distance,
                    &filter,
                )?;
                HostReply::RayHit(
                    hit.map(|hit| {
                        Ok::<GuestRayHit, ScriptError>(GuestRayHit {
                            entity: handles.export_token(hit.entity, self.max_handles)?,
                            position: hit.position.to_array(),
                            normal: hit.normal.to_array(),
                            distance: hit.distance,
                        })
                    })
                    .transpose()?,
                )
            }
            HostOperation::OverlapAabb {
                min,
                max,
                ignore,
                tag,
                triggers,
            } => {
                let filter = crate::query_api::QueryFilter {
                    ignore: if ignore.0 == 0 {
                        None
                    } else {
                        Some(handles.get(ignore)?)
                    },
                    tag,
                    include_triggers: triggers,
                };
                let hits =
                    ctx.overlap_aabb(Vec3::from_array(min), Vec3::from_array(max), &filter)?;
                HostReply::Entities(
                    hits.into_iter()
                        .map(|h| handles.export_token(h, self.max_handles))
                        .collect::<ScriptResult<Vec<_>>>()?,
                )
            }
            HostOperation::Owner => return handles.export(ctx.owner(), self.max_handles),
            HostOperation::Entity(name) => {
                if name.len() > 1024 {
                    return Err(ScriptError::InvalidArgument(
                        "Entity reference is too long".into(),
                    ));
                }
                return handles.export(ctx.entity(&name), self.max_handles);
            }
            HostOperation::EntityParameter(name) => {
                return handles.export(ctx.entity_parameter(&name)?, self.max_handles)
            }
            HostOperation::Parameter(name, default) => {
                HostReply::Value(ctx.parameter(&name, default)?)
            }
            HostOperation::IsValid(token) => HostReply::Value(ScriptValue::Bool(
                handles.get(token).is_ok_and(|h| h.is_valid(ctx)),
            )),
            HostOperation::Parent(token) => {
                let handle = handles.get(token)?;
                return handles.export(ctx.get_parent(handle)?, self.max_handles);
            }
            HostOperation::Child(token, name) => {
                if name.len() > 256 {
                    return Err(ScriptError::InvalidArgument(
                        "Child name is too long".into(),
                    ));
                }
                let handle = handles.get(token)?;
                return handles.export(ctx.find_child(handle, &name)?, self.max_handles);
            }
            HostOperation::Reparent {
                entity,
                parent,
                keep_world,
            } => {
                let parent = if parent.0 == 0 {
                    None
                } else {
                    Some(handles.get(parent)?)
                };
                handles.get(entity)?.reparent(ctx, parent, keep_world)?;
                none
            }
            HostOperation::WorldPosition(token) => {
                HostReply::Value(ScriptValue::Vec3(handles.get(token)?.get_position(ctx)?))
            }
            HostOperation::SetWorldPosition(token, [x, y, z]) => {
                handles.get(token)?.set_position(ctx, x, y, z)?;
                none
            }
            HostOperation::SetLocalPosition(token, value) => {
                handles
                    .get(token)?
                    .set_local_position(ctx, Vec3::from_array(value))?;
                none
            }
            HostOperation::SetLocalRotation(token, [x, y, z]) => {
                handles.get(token)?.set_rotation(ctx, x, y, z)?;
                none
            }
            HostOperation::ActiveCamera => {
                return handles.export(ctx.active_camera(), self.max_handles)
            }
            HostOperation::ActivateCamera(token) => {
                ctx.activate_camera(handles.get(token)?)?;
                none
            }
            HostOperation::ClearCamera => {
                ctx.clear_camera()?;
                none
            }
            HostOperation::HasCamera(token) => {
                let handle = handles.get(token)?;
                handle.resolve(ctx)?;
                HostReply::Value(ScriptValue::Bool(handle.camera_lens(ctx).is_ok()))
            }
            HostOperation::AddCamera(token) => {
                handles.get(token)?.add_camera(ctx)?;
                none
            }
            HostOperation::RemoveCamera(token) => {
                handles.get(token)?.remove_camera(ctx)?;
                none
            }
            HostOperation::SetFov(token, fov) => {
                let handle = handles.get(token)?;
                let mut lens = handle.camera_lens(ctx)?;
                lens.fov_degrees = fov;
                handle.set_camera_lens(ctx, lens)?;
                none
            }
            HostOperation::LookAt(token, point) => {
                handles
                    .get(token)?
                    .look_at(ctx, Vec3::from_array(point), Vec3::Y)?;
                none
            }
            HostOperation::Follow {
                camera,
                target,
                offset,
                aim,
                sharpness,
                dt,
            } => {
                handles.get(camera)?.follow(
                    ctx,
                    handles.get(target)?,
                    Vec3::from_array(offset),
                    Vec3::from_array(aim),
                    sharpness as f64,
                    dt as f64,
                )?;
                none
            }
            HostOperation::KeyHeld(key) => {
                if key.len() > 128 {
                    return Err(ScriptError::InvalidArgument("Key name is too long".into()));
                }
                HostReply::Value(ScriptValue::Bool(ctx.is_key_pressed(&key)))
            }
            HostOperation::IsClient => HostReply::Value(ScriptValue::Bool(
                ctx.view.role != crate::view::ScriptRole::Server,
            )),
            HostOperation::DeltaTime => HostReply::Value(ScriptValue::Float(ctx.delta_time())),
            HostOperation::Elapsed => HostReply::Value(ScriptValue::Float(ctx.elapsed())),
            HostOperation::GetProperty(token, name) => {
                if name.is_empty() || name.len() > 128 {
                    return Err(ScriptError::InvalidArgument("Invalid property name".into()));
                }
                let value = handles.get(token)?.get_property(ctx, &name)?;
                value.validate_event_payload()?;
                HostReply::Value(value)
            }
            HostOperation::SetProperty(token, name, value) => {
                value.validate_event_payload()?;
                handles.get(token)?.set_property(ctx, &name, value)?;
                none
            }
            HostOperation::SendEvent { target, name, data } => {
                let target = if target.0 == 0 {
                    None
                } else {
                    Some(handles.get(target)?)
                };
                ctx.emit_event(target, &name, data)?;
                none
            }
        })
    }
}

fn input_value(ctx: &ScriptContext<'_>, query: InputQuery) -> ScriptResult<ScriptValue> {
    let pair = |v: [f32; 2]| ScriptValue::List(v.into_iter().map(ScriptValue::Float).collect());
    let name = |key: &str| {
        if key.len() <= 128 {
            Ok(())
        } else {
            Err(ScriptError::InvalidArgument("input.name_limit".into()))
        }
    };
    let button = |b: i32| {
        if (0..=4).contains(&b) {
            Ok(())
        } else {
            Err(ScriptError::InvalidArgument("input.invalid_button".into()))
        }
    };
    Ok(match query {
        InputQuery::KeyPressed(key) => {
            name(&key)?;
            ScriptValue::Bool(ctx.input.binding_pressed(&key))
        }
        InputQuery::KeyReleased(key) => {
            name(&key)?;
            ScriptValue::Bool(ctx.input.binding_released(&key))
        }
        InputQuery::Axis(key) => {
            name(&key)?;
            ScriptValue::Float(
                ctx.input
                    .axes
                    .get(&key.to_ascii_lowercase())
                    .copied()
                    .unwrap_or(0.0),
            )
        }
        InputQuery::MouseHeld(b) => {
            button(b)?;
            ScriptValue::Bool(ctx.input.mouse_held.contains(&b))
        }
        InputQuery::MousePressed(b) => {
            button(b)?;
            ScriptValue::Bool(ctx.input.mouse_pressed.contains(&b))
        }
        InputQuery::MouseReleased(b) => {
            button(b)?;
            ScriptValue::Bool(ctx.input.mouse_released.contains(&b))
        }
        InputQuery::MousePosition => ctx.input.mouse_position.map(pair).unwrap_or_default(),
        InputQuery::MouseDelta => pair(ctx.input.mouse_delta),
        InputQuery::MouseRelative => pair(ctx.input.mouse_relative),
        InputQuery::MouseWheel => pair(ctx.input.mouse_wheel),
        InputQuery::CursorCapture => {
            ScriptValue::String(ctx.view.services.effective_cursor.name().into())
        }
    })
}
fn progress_value(
    state: &mut crate::services::ProgressState,
    operation: ProgressImport,
) -> ScriptResult<ScriptValue> {
    let none = ScriptValue::None;
    Ok(match operation {
        ProgressImport::Set(key, value) => {
            state.set(&key, value)?;
            none
        }
        ProgressImport::Get(key, fallback) => {
            fallback.validate_event_payload()?;
            state.get(&key, fallback)
        }
        ProgressImport::LoadedGet(key, fallback) => {
            fallback.validate_event_payload()?;
            state.loaded_get(&key, fallback)
        }
        ProgressImport::Remove(key) => {
            state.remove(&key)?;
            none
        }
        ProgressImport::Clear => {
            state.clear()?;
            none
        }
        ProgressImport::Discard => {
            state.discard()?;
            none
        }
        ProgressImport::Profile(profile) => {
            state.set_profile(&profile)?;
            none
        }
        ProgressImport::Save(slot) => ScriptValue::Int(state.request(&slot, false)? as i64),
        ProgressImport::Load(slot) => ScriptValue::Int(state.request(&slot, true)? as i64),
        ProgressImport::LoadBackup(slot) => ScriptValue::Int(state.request_backup(&slot)? as i64),
        ProgressImport::ApplyLoaded => {
            state.apply_loaded()?;
            none
        }
        ProgressImport::Status => state.report(),
    })
}
