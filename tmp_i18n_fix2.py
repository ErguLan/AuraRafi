import os

with open("crates/raf_core/src/event.rs", "r", encoding="utf-8") as f:
    text = f.read().replace("use std::any::{Any, TypeId};", "use std::any::Any;")
with open("crates/raf_core/src/event.rs", "w", encoding="utf-8") as f:
    f.write(text)

with open("crates/raf_nodes/src/executor.rs", "r", encoding="utf-8") as f:
    text = f.read().replace("let Some((src_node, src_pin))", "let Some((_src_node, src_pin))")
with open("crates/raf_nodes/src/executor.rs", "w", encoding="utf-8") as f:
    f.write(text)

with open("crates/raf_electronics/src/drc.rs", "r", encoding="utf-8") as f:
    text = f.read().replace("use crate::component::{ElectronicComponent, SimModel};", "use crate::component::SimModel;")
with open("crates/raf_electronics/src/drc.rs", "w", encoding="utf-8") as f:
    f.write(text)

with open("crates/raf_electronics/src/export.rs", "r", encoding="utf-8") as f:
    lines = [l for l in f.readlines() if "use crate::component::ElectronicComponent;" not in l]
with open("crates/raf_electronics/src/export.rs", "w", encoding="utf-8") as f:
    f.writelines(lines)

with open("crates/raf_electronics/src/netlist.rs", "r", encoding="utf-8") as f:
    lines = [l for l in f.readlines() if "use crate::component::ElectronicComponent;" not in l]
with open("crates/raf_electronics/src/netlist.rs", "w", encoding="utf-8") as f:
    text = "".join(lines).replace("use crate::schematic::{Schematic, Wire};", "use crate::schematic::Schematic;")
    text = text.replace("const GRID_STEP: f32 = 20.0;", "const _GRID_STEP: f32 = 20.0;")
    f.write(text)

with open("crates/raf_electronics/src/simulation.rs", "r", encoding="utf-8") as f:
    lines = [l for l in f.readlines() if "use uuid::Uuid;" not in l]
with open("crates/raf_electronics/src/simulation.rs", "w", encoding="utf-8") as f:
    f.writelines(lines)

with open("crates/raf_editor/src/panels/schematic_view.rs", "r", encoding="utf-8") as f:
    text = f.read()
    text = text.replace("let lang = self.lang;", "let _lang = self.lang;")
    text = text.replace("let netlist = self.schematic.netlist();", "let _netlist = self.schematic.netlist();")
    text = text.replace("for (wi, wire) in", "for (_wi, wire) in")
with open("crates/raf_editor/src/panels/schematic_view.rs", "w", encoding="utf-8") as f:
    f.write(text)

with open("crates/raf_editor/src/app.rs", "r", encoding="utf-8") as f:
    text = f.read().replace("use raf_core::config::{EngineSettings, Language, Theme};", "use raf_core::config::{EngineSettings, Theme};")
with open("crates/raf_editor/src/app.rs", "w", encoding="utf-8") as f:
    f.write(text)

with open("crates/raf_editor/src/panels/viewport.rs", "r", encoding="utf-8") as f:
    text = f.read().replace("use raf_render::camera::{Camera, CameraMode};", "use raf_render::camera::Camera;")
with open("crates/raf_editor/src/panels/viewport.rs", "w", encoding="utf-8") as f:
    f.write(text)
