//! Transient host state for the retained Assets surface.
//!
//! The surface remains declarative; this host keeps only interaction state
//! needed to rebuild it and leaves filesystem and scene mutations at the
//! application boundary.

use super::assets_surface::AssetFilter;

#[derive(Debug, Clone)]
pub struct AssetsSurfaceHost {
    pub(crate) query: String,
    pub(crate) filter: AssetFilter,
    pub(crate) script_menu_open: bool,
    pub(crate) script_name: String,
    pub(crate) file_menu_open: bool,
    pub(crate) file_name: String,
    pub(crate) refresh_requested: bool,
    pub(crate) primitive_menu_open: bool,
}

impl Default for AssetsSurfaceHost {
    fn default() -> Self {
        Self {
            query: String::new(),
            filter: AssetFilter::All,
            script_menu_open: false,
            script_name: "new_script".to_string(),
            file_menu_open: false,
            file_name: "new_file".to_string(),
            refresh_requested: false,
            primitive_menu_open: false,
        }
    }
}
