//! Typed imports delegate to the existing Host API, never to scene internals.
use crate::{NodeHandle, ScriptContext, ScriptError, ScriptResult, ScriptValue};
use glam::Vec3;

/// Zero is unassigned. Guest tokens are table entries, NOT raw SceneNodeId values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestEntity(pub u32);

pub enum HostOperation {
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
