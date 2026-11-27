use std::sync::OnceLock;

use crate::{Error, Result};

/// Parsed and validated main scene shader.
///
/// Device and surface recreation happens whenever the editor reopens its
/// window, but the WGSL source never changes at runtime. Parsing and validating
/// it once per process removes that work from every later device startup.
static SCENE_SHADER: OnceLock<
    std::result::Result<(naga::Module, naga::valid::ModuleInfo), String>,
> = OnceLock::new();

pub(super) fn source() -> Result<(naga::Module, naga::valid::ModuleInfo)> {
    match SCENE_SHADER.get_or_init(|| parse(crate::SHADER).map_err(|error| error.to_string())) {
        Ok((module, info)) => Ok((module.clone(), info.clone())),
        Err(error) => Err(Error::Backend(error.clone())),
    }
}

pub(super) fn parse(source: &str) -> Result<(naga::Module, naga::valid::ModuleInfo)> {
    let module = naga::front::wgsl::parse_str(source)
        .map_err(|error| Error::Backend(error.emit_to_string(source)))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|error| Error::Backend(error.to_string()))?;
    Ok((module, info))
}
