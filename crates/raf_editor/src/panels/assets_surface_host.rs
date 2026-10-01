//! Transient host state for the retained Assets surface.
//!
//! The surface remains declarative; this host keeps only interaction state
//! needed to rebuild it and leaves filesystem and scene mutations at the
//! application boundary.

use super::assets_surface::{
    AssetFilter, AssetSort, AssetViewMode, AssetsOperation, AssetsStatus,
};

/// Pointer-anchored context menu for a single asset row.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetsContextMenu {
    pub row: String,
    pub builtin: bool,
    pub position: [f32; 2],
}

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
    pub(crate) create_menu_open: bool,
    pub(crate) script_extension: String,
    pub(crate) sort: AssetSort,
    pub(crate) view: AssetViewMode,
    pub(crate) selected_asset: Option<String>,
    pub(crate) highlight_asset: Option<String>,
    pub(crate) status: Option<AssetsStatus>,
    pub(crate) operation: Option<AssetsOperation>,
    pub(crate) open_asset: Option<String>,
    pub(crate) rename_asset: Option<String>,
    pub(crate) rename_value: String,
    pub(crate) delete_asset: Option<String>,
    pub(crate) context_menu: Option<AssetsContextMenu>,
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
            create_menu_open: false,
            script_extension: String::new(),
            sort: AssetSort::NameAsc,
            view: AssetViewMode::Grid,
            selected_asset: None,
            highlight_asset: None,
            status: None,
            operation: None,
            open_asset: None,
            rename_asset: None,
            rename_value: String::new(),
            delete_asset: None,
            context_menu: None,
        }
    }
}

impl AssetsSurfaceHost {
    /// True while any Assets overlay (menus, popovers, modals) is open. The
    /// workbench drives a single entrance tween from this so every overlay
    /// shares one motion curve instead of rebuilding per pixel.
    pub(crate) fn has_open_overlay(&self) -> bool {
        self.create_menu_open
            || self.script_menu_open
            || self.file_menu_open
            || self.primitive_menu_open
            || self.open_asset.is_some()
            || self.rename_asset.is_some()
            || self.delete_asset.is_some()
            || self.context_menu.is_some()
    }

    /// True while a transient Assets menu or popover is open (not a modal).
    pub(crate) fn has_open_menu(&self) -> bool {
        self.create_menu_open
            || self.script_menu_open
            || self.file_menu_open
            || self.primitive_menu_open
            || self.context_menu.is_some()
    }

    pub(crate) fn close_menus(&mut self) -> bool {
        let was_open = self.has_open_menu();
        self.create_menu_open = false;
        self.script_menu_open = false;
        self.file_menu_open = false;
        self.primitive_menu_open = false;
        self.context_menu = None;
        was_open
    }

    pub(crate) fn close_modals(&mut self) -> bool {
        let was_open = self.open_asset.is_some()
            || self.rename_asset.is_some()
            || self.delete_asset.is_some();
        self.open_asset = None;
        self.rename_asset = None;
        self.delete_asset = None;
        was_open
    }

    /// True when `target` belongs to an Assets overlay, so an outside-click
    /// pass must not close it before the overlay itself handles the press.
    pub(crate) fn is_overlay_target(target: &str) -> bool {
        const PREFIXES: [&str; 12] = [
            "assets.create-trigger",
            "assets.create-menu",
            "assets.script-popover",
            "assets.file-popover",
            "assets.primitive-popover",
            "assets.context",
            "assets.modal",
            "assets.backdrop",
            "assets.rename",
            "assets.delete",
            "assets.open-modal",
            "assets.status",
        ];
        PREFIXES.iter().any(|prefix| target.starts_with(prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_a_menu_closes_the_other_overlays() {
        let mut host = AssetsSurfaceHost::default();
        host.create_menu_open = true;
        host.script_menu_open = true;
        host.context_menu = Some(AssetsContextMenu {
            row: "scripts/player.rs".to_string(),
            builtin: false,
            position: [10.0, 20.0],
        });

        assert!(host.close_menus());
        assert!(!host.has_open_menu());
        assert!(!host.create_menu_open);
        assert!(!host.script_menu_open);
        assert!(host.context_menu.is_none());
        assert!(!host.close_menus());
    }

    #[test]
    fn modals_and_menus_report_overlay_state_separately() {
        let mut host = AssetsSurfaceHost::default();
        assert!(!host.has_open_overlay());

        host.open_asset = Some("scripts/player.rs".to_string());
        assert!(host.has_open_overlay());
        assert!(!host.has_open_menu());

        assert!(host.close_modals());
        assert!(host.open_asset.is_none());
        assert!(!host.has_open_overlay());
    }

    #[test]
    fn overlay_targets_keep_the_press_inside_the_overlay() {
        assert!(AssetsSurfaceHost::is_overlay_target(
            "assets.create-menu.script"
        ));
        assert!(AssetsSurfaceHost::is_overlay_target(
            "assets.context.copy-path"
        ));
        assert!(AssetsSurfaceHost::is_overlay_target("assets.backdrop"));
        assert!(!AssetsSurfaceHost::is_overlay_target("assets.card.3"));
        assert!(!AssetsSurfaceHost::is_overlay_target("assets.grid"));
    }
}
