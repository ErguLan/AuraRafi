//! Single semantic icon manifest shared by the Rust API and development tooling.
//!
//! The one-argument callback preserves the original `Variant => key` contract.
//! Metadata-aware consumers pass `metadata` as the second argument.
#[macro_export]
macro_rules! ui_icon_catalog {
    ($consumer:ident) => {
        $crate::ui_icon_catalog!(@dispatch $consumer, legacy);
    };
    ($consumer:ident, metadata) => {
        $crate::ui_icon_catalog!(@dispatch $consumer, metadata);
    };
    (@dispatch $consumer:ident, $mode:ident) => {
        $crate::ui_icon_catalog!(@emit $consumer, $mode; [
            Select { key: "select", master_svg: "crates/raf_render/assets/ui_icons/svg/select.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Select"] },
            Move { key: "move", master_svg: "crates/raf_render/assets/ui_icons/svg/move.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Move"] },
            Rotate { key: "rotate", master_svg: "crates/raf_render/assets/ui_icons/svg/rotate.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Rotate"] },
            Scale { key: "scale", master_svg: "crates/raf_render/assets/ui_icons/svg/scale.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Scale"] },
            Focus { key: "focus", master_svg: "crates/raf_render/assets/ui_icons/svg/focus.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Focus"] },
            Undo { key: "undo", master_svg: "crates/raf_render/assets/ui_icons/svg/undo.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Undo"] },
            Redo { key: "redo", master_svg: "crates/raf_render/assets/ui_icons/svg/redo.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Redo"] },
            Refresh { key: "refresh", master_svg: "crates/raf_render/assets/ui_icons/svg/refresh.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Refresh"] },
            Grid { key: "grid", master_svg: "crates/raf_render/assets/ui_icons/svg/grid.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Grid"] },
            View2d { key: "view-2d", master_svg: "crates/raf_render/assets/ui_icons/svg/view-2d.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["View2d"] },
            View3d { key: "view-3d", master_svg: "crates/raf_render/assets/ui_icons/svg/view-3d.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["View3d"] },
            ScenePreview { key: "scene-preview", master_svg: "crates/raf_render/assets/ui_icons/svg/scene-preview.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["ScenePreview"] },
            Shaded { key: "shaded", master_svg: "crates/raf_render/assets/ui_icons/svg/shaded.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Shaded"] },
            Wireframe { key: "wireframe", master_svg: "crates/raf_render/assets/ui_icons/svg/wireframe.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Wireframe"] },
            CadRoute { key: "cad-route", master_svg: "crates/raf_render/assets/ui_icons/svg/cad-route.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 22, aliases: ["CadRoute"] },
            CadOutline { key: "cad-outline", master_svg: "crates/raf_render/assets/ui_icons/svg/cad-outline.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 22, aliases: ["CadOutline"] },
            CadPlace { key: "cad-place", master_svg: "crates/raf_render/assets/ui_icons/svg/cad-place.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 22, aliases: ["CadPlace"] },
            CadVia { key: "cad-via", master_svg: "crates/raf_render/assets/ui_icons/svg/cad-via.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 22, aliases: ["CadVia"] },
            Folder { key: "folder", master_svg: "crates/raf_render/assets/ui_icons/svg/folder.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Folder"] },
            Scene { key: "scene", master_svg: "crates/raf_render/assets/ui_icons/svg/scene.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Scene"] },
            Entity { key: "entity", master_svg: "crates/raf_render/assets/ui_icons/svg/entity.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Entity"] },
            Camera { key: "camera", master_svg: "crates/raf_render/assets/ui_icons/svg/camera.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Camera"] },
            Sun { key: "sun", master_svg: "crates/raf_render/assets/ui_icons/svg/sun.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Sun"] },
            Sky { key: "sky", master_svg: "crates/raf_render/assets/ui_icons/svg/sky.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Sky"] },
            Shadows { key: "shadows", master_svg: "crates/raf_render/assets/ui_icons/svg/shadows.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Shadows"] },
            Clock { key: "clock", master_svg: "crates/raf_render/assets/ui_icons/svg/clock.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Clock"] },
            Cube { key: "cube", master_svg: "crates/raf_render/assets/ui_icons/svg/cube.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Cube"] },
            Sphere { key: "sphere", master_svg: "crates/raf_render/assets/ui_icons/svg/sphere.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Sphere"] },
            Plane { key: "plane", master_svg: "crates/raf_render/assets/ui_icons/svg/plane.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Plane"] },
            Cylinder { key: "cylinder", master_svg: "crates/raf_render/assets/ui_icons/svg/cylinder.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Cylinder"] },
            Capsule { key: "capsule", master_svg: "crates/raf_render/assets/ui_icons/svg/capsule.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Capsule"] },
            Eye { key: "eye", master_svg: "crates/raf_render/assets/ui_icons/svg/eye.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Eye"] },
            EyeOff { key: "eye-off", master_svg: "crates/raf_render/assets/ui_icons/svg/eye-off.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["EyeOff"] },
            Lock { key: "lock", master_svg: "crates/raf_render/assets/ui_icons/svg/lock.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Lock"] },
            Unlock { key: "unlock", master_svg: "crates/raf_render/assets/ui_icons/svg/unlock.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Unlock"] },
            ChevronLeft { key: "chevron-left", master_svg: "crates/raf_render/assets/ui_icons/svg/chevron-left.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["ChevronLeft"] },
            ChevronRight { key: "chevron-right", master_svg: "crates/raf_render/assets/ui_icons/svg/chevron-right.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["ChevronRight"] },
            ChevronDown { key: "chevron-down", master_svg: "crates/raf_render/assets/ui_icons/svg/chevron-down.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["ChevronDown"] },
            More { key: "more", master_svg: "crates/raf_render/assets/ui_icons/svg/more.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["More"] },
            Search { key: "search", master_svg: "crates/raf_render/assets/ui_icons/svg/search.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Search"] },
            Filter { key: "filter", master_svg: "crates/raf_render/assets/ui_icons/svg/filter.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Filter"] },
            Add { key: "add", master_svg: "crates/raf_render/assets/ui_icons/svg/add.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Add"] },
            Close { key: "close", master_svg: "crates/raf_render/assets/ui_icons/svg/close.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Close"] },
            Play { key: "play", master_svg: "crates/raf_render/assets/ui_icons/svg/play.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Play"] },
            Pause { key: "pause", master_svg: "crates/raf_render/assets/ui_icons/svg/pause.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Pause"] },
            StepForward { key: "step-forward", master_svg: "crates/raf_render/assets/ui_icons/svg/step-forward.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["StepForward"] },
            Stop { key: "stop", master_svg: "crates/raf_render/assets/ui_icons/svg/stop.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Stop"] },
            Console { key: "console", master_svg: "crates/raf_render/assets/ui_icons/svg/console.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Console"] },
            Assets { key: "assets", master_svg: "crates/raf_render/assets/ui_icons/svg/assets.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Assets"] },
            Project { key: "project", master_svg: "crates/raf_render/assets/ui_icons/svg/project.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Project"] },
            Node { key: "node", master_svg: "crates/raf_render/assets/ui_icons/svg/node.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Node"] },
            Agent { key: "agent", master_svg: "crates/raf_render/assets/ui_icons/svg/agent.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Agent"] },
            Schematic { key: "schematic", master_svg: "crates/raf_render/assets/ui_icons/svg/schematic.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Schematic"] },
            Pcb { key: "pcb", master_svg: "crates/raf_render/assets/ui_icons/svg/pcb.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Pcb"] },
            Wire { key: "wire", master_svg: "crates/raf_render/assets/ui_icons/svg/wire.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Wire"] },
            Route { key: "route", master_svg: "crates/raf_render/assets/ui_icons/svg/route.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Route"] },
            BoardOutline { key: "board-outline", master_svg: "crates/raf_render/assets/ui_icons/svg/board-outline.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["BoardOutline"] },
            ZoomIn { key: "zoom-in", master_svg: "crates/raf_render/assets/ui_icons/svg/zoom-in.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["ZoomIn"] },
            ZoomOut { key: "zoom-out", master_svg: "crates/raf_render/assets/ui_icons/svg/zoom-out.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["ZoomOut"] },
            Trash { key: "trash", master_svg: "crates/raf_render/assets/ui_icons/svg/trash.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Trash"] },
            Script { key: "script", master_svg: "crates/raf_render/assets/ui_icons/svg/script.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Script"] },
            File { key: "file", master_svg: "crates/raf_render/assets/ui_icons/svg/file.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["File"] },
            ExternalLink { key: "external-link", master_svg: "crates/raf_render/assets/ui_icons/svg/external-link.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["ExternalLink"] },
            Pencil { key: "pencil", master_svg: "crates/raf_render/assets/ui_icons/svg/pencil.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Pencil"] },
            Copy { key: "copy", master_svg: "crates/raf_render/assets/ui_icons/svg/copy.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Copy"] },
            Settings { key: "settings", master_svg: "crates/raf_render/assets/ui_icons/svg/settings.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Settings"] },
            Menu { key: "menu", master_svg: "crates/raf_render/assets/ui_icons/svg/menu.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Menu"] },
            Warning { key: "warning", master_svg: "crates/raf_render/assets/ui_icons/svg/warning.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Warning"] },
            Error { key: "error", master_svg: "crates/raf_render/assets/ui_icons/svg/error.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Error"] },
            Success { key: "success", master_svg: "crates/raf_render/assets/ui_icons/svg/success.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Success"] },
            Magnet { key: "magnet", master_svg: "crates/raf_render/assets/ui_icons/svg/magnet.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Magnet"] },
            Frame { key: "frame", master_svg: "crates/raf_render/assets/ui_icons/svg/frame.svg", source_raster_size: [64, 64], source_canvas_size: [64, 64], optical_size_px: 18, aliases: ["Frame"] },
        ]);
    };
    (@emit $consumer:ident, legacy; [$($variant:ident {
        key: $key:literal,
        master_svg: $master_svg:literal,
        source_raster_size: [$raster_width:literal, $raster_height:literal],
        source_canvas_size: [$canvas_width:literal, $canvas_height:literal],
        optical_size_px: $optical_size_px:literal,
        aliases: [$($alias:literal),* $(,)?] $(,)?
    }),* $(,)?]) => {
        $consumer! { $($variant => $key,)* }
    };
    (@emit $consumer:ident, metadata; [$($variant:ident {
        key: $key:literal,
        master_svg: $master_svg:literal,
        source_raster_size: [$raster_width:literal, $raster_height:literal],
        source_canvas_size: [$canvas_width:literal, $canvas_height:literal],
        optical_size_px: $optical_size_px:literal,
        aliases: [$($alias:literal),* $(,)?] $(,)?
    }),* $(,)?]) => {
        $consumer! {
            $($variant => {
                key: $key,
                master_svg: $master_svg,
                source_raster_size: [$raster_width, $raster_height],
                source_canvas_size: [$canvas_width, $canvas_height],
                optical_size_px: $optical_size_px,
                aliases: [$($alias),*],
            },)*
        }
    };
}
