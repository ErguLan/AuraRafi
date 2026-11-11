//! Client presentation API. It cannot address editor chrome or mutate world authority.
use crate::{ScriptContext, ScriptError, ScriptResult};
pub use raf_ui::game_ui::{UiElementRef, UiMount, UiPresentationKind};

fn error(reason: String) -> ScriptError {
    ScriptError::InvalidArgument(reason)
}

/// Language adapters use these operations instead of exposing RafUI or renderer internals.
pub enum UiImport {
    Text {
        instance: String,
        node: String,
        value: String,
    },
    Image {
        instance: String,
        node: String,
        source: String,
    },
    Visible {
        instance: String,
        node: String,
        value: bool,
    },
    Enabled {
        instance: String,
        node: String,
        value: bool,
    },
    Show {
        instance: String,
        value: bool,
    },
    Value {
        key: String,
        value: String,
    },
    Clone {
        source: String,
        id: String,
    },
    Close {
        id: String,
    },
    Mount {
        instance: String,
        mount: UiMount,
    },
}
pub fn dispatch_import(
    ctx: &mut ScriptContext<'_>,
    operation: UiImport,
) -> ScriptResult<crate::ScriptValue> {
    match operation {
        UiImport::Text {
            instance,
            node,
            value,
        } => {
            let element = ctx.ui_element(&instance, &node)?;
            ctx.ui_text(&element, &value)?;
        }
        UiImport::Image {
            instance,
            node,
            source,
        } => {
            let element = ctx.ui_element(&instance, &node)?;
            ctx.ui_image(&element, &source)?;
        }
        UiImport::Visible {
            instance,
            node,
            value,
        } => {
            let element = ctx.ui_element(&instance, &node)?;
            ctx.ui_visible(&element, value)?;
        }
        UiImport::Enabled {
            instance,
            node,
            value,
        } => {
            let element = ctx.ui_element(&instance, &node)?;
            ctx.ui_enabled(&element, value)?;
        }
        UiImport::Show { instance, value } => ctx.ui_show(&instance, value)?,
        UiImport::Value { key, value } => ctx.ui_value(&key, &value)?,
        UiImport::Clone { source, id } => ctx.ui_clone(&source, &id)?,
        UiImport::Close { id } => return Ok(crate::ScriptValue::Bool(ctx.ui_close(&id)?)),
        UiImport::Mount { instance, mount } => ctx.ui_mount(&instance, mount)?,
    }
    Ok(crate::ScriptValue::None)
}
impl ScriptContext<'_> {
    fn check_ui_role(&self) -> ScriptResult<()> {
        self.check_budget()?;
        if self.view.role == crate::view::ScriptRole::Server {
            return Err(error("ui.client_only".into()));
        }
        Ok(())
    }
    pub fn ui_element(&self, instance: &str, node: &str) -> ScriptResult<UiElementRef> {
        self.check_ui_role()?;
        self.view.game_ui.element(instance, node).map_err(error)
    }
    pub fn ui_text(&mut self, element: &UiElementRef, text: &str) -> ScriptResult<()> {
        self.check_ui_role()?;
        if text.len() > 4096 {
            return Err(error("ui.value_budget".into()));
        }
        self.view
            .game_ui
            .update(element, raf_ui::UiDocumentUpdate::TextValue(text.into()))
            .map_err(error)
    }
    pub fn ui_visible(&mut self, element: &UiElementRef, visible: bool) -> ScriptResult<()> {
        self.check_ui_role()?;
        self.view
            .game_ui
            .update(element, raf_ui::UiDocumentUpdate::Visible(visible))
            .map_err(error)
    }
    pub fn ui_enabled(&mut self, element: &UiElementRef, enabled: bool) -> ScriptResult<()> {
        self.check_ui_role()?;
        self.view
            .game_ui
            .update(element, raf_ui::UiDocumentUpdate::Enabled(enabled))
            .map_err(error)
    }
    pub fn ui_image(&mut self, element: &UiElementRef, source: &str) -> ScriptResult<()> {
        self.check_ui_role()?;
        if source.len() > 512 {
            return Err(error("ui.value_budget".into()));
        }
        self.view
            .game_ui
            .update(
                element,
                raf_ui::UiDocumentUpdate::Image(raf_ui::UiImage {
                    source: raf_ui::UiImageSource::new(source),
                    fit: raf_ui::UiImageFit::Contain,
                    tint: None,
                    options: Default::default(),
                }),
            )
            .map_err(error)
    }
    pub fn ui_show(&mut self, instance: &str, visible: bool) -> ScriptResult<()> {
        self.check_ui_role()?;
        self.view.game_ui.show(instance, visible).map_err(error)
    }
    pub fn ui_clone(&mut self, source: &str, id: &str) -> ScriptResult<()> {
        self.check_ui_role()?;
        self.view.game_ui.clone_instance(source, id).map_err(error)
    }
    pub fn ui_close(&mut self, id: &str) -> ScriptResult<bool> {
        self.check_ui_role()?;
        Ok(self.view.game_ui.close(id))
    }
    pub fn ui_value(&mut self, key: &str, value: &str) -> ScriptResult<()> {
        self.check_ui_role()?;
        self.view.game_ui.set_value(key, value).map_err(error)
    }
    pub fn ui_mount(&mut self, id: &str, mount: UiMount) -> ScriptResult<()> {
        self.check_ui_role()?;
        self.view.game_ui.set_mount(id, mount).map_err(error)
    }
    pub fn ui_create(
        &mut self,
        instance: &str,
        parent: &str,
        id: &str,
        kind: &str,
    ) -> ScriptResult<UiElementRef> {
        self.check_ui_role()?;
        let kind = raf_ui::authoring::UiElementKind::parse(kind)
            .ok_or_else(|| error("ui.invalid_element".into()))?;
        self.view
            .game_ui
            .edit(
                instance,
                &[raf_ui::authoring::UiEdit::Insert {
                    parent: parent.into(),
                    id: id.into(),
                    kind,
                    rect: None,
                    text: String::new(),
                }],
            )
            .map_err(error)?;
        self.view.game_ui.element(instance, id).map_err(error)
    }
    pub fn ui_remove(&mut self, element: &UiElementRef) -> ScriptResult<()> {
        self.check_ui_role()?;
        let reference = self
            .view
            .game_ui
            .element(&element.instance, &element.node)
            .map_err(error)?;
        if reference.generation != element.generation {
            return Err(error("ui.stale_reference".into()));
        }
        self.view
            .game_ui
            .edit(
                &element.instance,
                &[raf_ui::authoring::UiEdit::Remove {
                    node: element.node.clone(),
                }],
            )
            .map_err(error)
    }
    pub fn ui_rect(&mut self, element: &UiElementRef, rect: raf_ui::UiRect) -> ScriptResult<()> {
        self.check_ui_role()?;
        let reference = self
            .view
            .game_ui
            .element(&element.instance, &element.node)
            .map_err(error)?;
        if reference.generation != element.generation {
            return Err(error("ui.stale_reference".into()));
        }
        self.view
            .game_ui
            .edit(
                &element.instance,
                &[raf_ui::authoring::UiEdit::Patch {
                    node: element.node.clone(),
                    patch: raf_ui::authoring::UiNodePatch {
                        layout: Some(raf_ui::UiLayout::absolute(rect)),
                        ..Default::default()
                    },
                }],
            )
            .map_err(error)
    }
}
