use crate::{Error, Result};

pub(super) fn source() -> Result<(naga::Module, naga::valid::ModuleInfo)> {
    parse(crate::SHADER)
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
