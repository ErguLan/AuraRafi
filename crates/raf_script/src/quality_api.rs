//! Shared graphics-quality Host API. Overrides stay local to one runtime view.
use crate::view::{geometry_detail_slug, texture_quality_slug, viewport_resolution_slug};
use crate::{ScriptContext, ScriptError, ScriptResult, ScriptValue};
use std::collections::BTreeMap;

fn invalid(message: impl Into<String>) -> ScriptError {
    ScriptError::InvalidArgument(message.into())
}

impl ScriptContext<'_> {
    /// Effective quality plus the project-authored base it is layered over.
    pub fn get_quality(&self) -> ScriptResult<ScriptValue> {
        let quality = self.view.quality();
        let base = self.view.quality_base;
        Ok(ScriptValue::Record(BTreeMap::from([
            (
                "viewport_resolution".into(),
                ScriptValue::String(viewport_resolution_slug(quality.viewport_resolution).into()),
            ),
            (
                "geometry_detail".into(),
                ScriptValue::String(geometry_detail_slug(quality.geometry_detail).into()),
            ),
            (
                "texture_quality".into(),
                ScriptValue::String(texture_quality_slug(quality.texture_quality).into()),
            ),
            (
                "base_viewport_resolution".into(),
                ScriptValue::String(viewport_resolution_slug(base.viewport_resolution).into()),
            ),
            (
                "base_geometry_detail".into(),
                ScriptValue::String(geometry_detail_slug(base.geometry_detail).into()),
            ),
            (
                "base_texture_quality".into(),
                ScriptValue::String(texture_quality_slug(base.texture_quality).into()),
            ),
        ])))
    }

    pub fn set_quality(&mut self, path: &str, value: ScriptValue) -> ScriptResult<()> {
        self.check_budget()?;
        let ScriptValue::String(value) = value else {
            return Err(invalid("set_quality expects a string choice"));
        };
        self.view
            .set_quality_override(path, &value)
            .map_err(invalid)?;
        Ok(())
    }

    pub fn clear_quality(&mut self) -> ScriptResult<()> {
        self.check_budget()?;
        self.view.clear_quality();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::rhai_backend as rhai;
    use crate::view::RuntimeViewState;
    use crate::{host_api::ScriptEventQueue, AudioCommandQueue, InputSnapshot, TimeInfo};
    use raf_core::project::{GeometryDetailMode, TextureQualityMode, ViewportResolutionMode};

    #[test]
    fn quality_rhai_and_host_api_share_validation_and_only_bump_on_change() {
        let engine = rhai::create_engine(100_000);
        let mut scene = raf_core::SceneGraph::new();
        let mut view = RuntimeViewState::default();
        let input = InputSnapshot::default();
        let mut audio = AudioCommandQueue::default();
        let mut events = ScriptEventQueue::default();
        let mut ctx = ScriptContext {
            scene: &mut scene,
            view: &mut view,
            input: &input,
            audio: &mut audio,
            events: &mut events,
            time: TimeInfo::default(),
            instance_id: 1,
            owner: None,
            behavior_source: None,
            entity_limit: 1000,
            cancellation: None,
            deadline: None,
        };
        let script = rhai::compile_source(
            &engine,
            "quality.rhai",
            r#"
            let q = get_quality();
            if q.viewport_resolution != "adaptive" { throw "default viewport"; }
            if q.base_geometry_detail != "adaptive" { throw "base geometry"; }
            set_quality("viewport_resolution", "efficient");
            set_quality("geometry_detail", "detailed");
            set_quality("texture_quality", "efficient");
            let q = get_quality();
            if q.viewport_resolution != "efficient" { throw "viewport override"; }
            if q.geometry_detail != "detailed" { throw "geometry override"; }
            if q.texture_quality != "efficient" { throw "texture override"; }
            if q.base_texture_quality != "adaptive" { throw "base changed"; }
            clear_quality();
            let q = get_quality();
            if q.viewport_resolution != "adaptive" { throw "clear viewport"; }
            if q.geometry_detail != "adaptive" { throw "clear geometry"; }
            if q.texture_quality != "adaptive" { throw "clear texture"; }
        "#,
        )
        .unwrap();
        let result = rhai::initialize_scope(&engine, &script, &mut ctx, &mut ::rhai::Scope::new());
        assert!(result.success, "{:?}", result.errors);
        assert_eq!(ctx.view.quality(), Default::default());
        assert_eq!(ctx.view.quality_revision(), 4);

        let revision = ctx.view.quality_revision();
        assert!(!ctx
            .view
            .set_quality_override("viewport_resolution", "adaptive")
            .unwrap());
        assert_eq!(ctx.view.quality_revision(), revision);
        assert!(ctx
            .view
            .set_quality_override("viewport_resolution", "native")
            .unwrap());
        assert_eq!(
            ctx.view.quality().viewport_resolution,
            ViewportResolutionMode::FullResolution
        );
        assert_eq!(ctx.view.quality_revision(), revision + 1);
        assert!(ctx.view.clear_quality());
        assert_eq!(ctx.view.quality_revision(), revision + 2);
        assert!(!ctx.view.clear_quality());
        assert_eq!(ctx.view.quality_revision(), revision + 2);

        assert!(ctx
            .set_quality("geometry_detail", ScriptValue::String("efficient".into()))
            .is_ok());
        assert_eq!(
            ctx.view.quality().geometry_detail,
            GeometryDetailMode::Efficient
        );
        assert!(ctx
            .set_quality("texture_quality", ScriptValue::String("detailed".into()))
            .is_ok());
        assert_eq!(
            ctx.view.quality().texture_quality,
            TextureQualityMode::Detailed
        );
        assert!(ctx.view.clear_quality());
        assert_eq!(ctx.view.quality(), Default::default());

        for (path, value) in [
            ("viewport_resolution", "full"),
            ("geometry_detail", "native"),
            ("texture_quality", "high"),
            ("resolution", "adaptive"),
        ] {
            assert!(ctx
                .set_quality(path, ScriptValue::String(value.into()))
                .is_err());
        }
        assert!(ctx
            .set_quality("viewport_resolution", ScriptValue::Int(1))
            .is_err());
        assert_eq!(ctx.view.quality(), Default::default());
        let base = raf_core::project::ProjectSettings {
            viewport_resolution: ViewportResolutionMode::Efficient,
            geometry_detail: GeometryDetailMode::Detailed,
            texture_quality: TextureQualityMode::Efficient,
            ..Default::default()
        };
        ctx.view.quality_base = crate::view::RuntimeQuality::from_settings(&base);
        let q = ctx.get_quality().unwrap();
        assert_eq!(
            q,
            ScriptValue::Record(BTreeMap::from([
                (
                    "viewport_resolution".into(),
                    ScriptValue::String("efficient".into())
                ),
                (
                    "geometry_detail".into(),
                    ScriptValue::String("detailed".into())
                ),
                (
                    "texture_quality".into(),
                    ScriptValue::String("efficient".into())
                ),
                (
                    "base_viewport_resolution".into(),
                    ScriptValue::String("efficient".into())
                ),
                (
                    "base_geometry_detail".into(),
                    ScriptValue::String("detailed".into())
                ),
                (
                    "base_texture_quality".into(),
                    ScriptValue::String("efficient".into())
                ),
            ]))
        );
    }
}
