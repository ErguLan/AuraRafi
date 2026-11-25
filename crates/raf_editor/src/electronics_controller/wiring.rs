//! Automatic net naming and wire anchor identity.

use super::*;

impl NativeElectronicsEditor {
    /// Next automatic net name.
    ///
    /// The old implementation derived the counter from the wire count. Because
    /// one wire gesture creates several `Wire` entries and deleting a wire
    /// lowers the count, it reused names that were still alive and merged two
    /// different circuits into one net. The counter is now the highest numeric
    /// suffix that exists right now, across wires, the live netlist and the PCB.
    pub(super) fn next_net_name(&self) -> String {
        let mut highest = 0usize;
        let mut observe = |name: &str| {
            if let Some(value) = net_name_suffix(name) {
                highest = highest.max(value);
            }
        };
        for wire in &self.schematic.wires {
            observe(&wire.net);
        }
        for net in &self.schematic.netlist().nets {
            observe(&net.name);
        }
        for trace in &self.pcb.traces {
            observe(&trace.net);
        }
        for airwire in &self.pcb.airwires {
            observe(&airwire.net);
        }
        for placement in &self.pcb.components {
            for net in &placement.pad_nets {
                observe(net);
            }
        }
        format!(
            "{NET_NAME_PREFIX}{:0width$}",
            highest + 1,
            width = NET_NAME_DIGITS
        )
    }
}

/// Numeric suffix of an automatic net name, or `None` for user labels.
fn net_name_suffix(name: &str) -> Option<usize> {
    let rest = name.strip_prefix(NET_NAME_PREFIX)?;
    if rest.is_empty() || !rest.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

pub(super) fn anchor_component_id(anchor: WireAnchor) -> Uuid {
    match anchor {
        WireAnchor::Pin { component_id, .. } => component_id,
        WireAnchor::Point(_) => Uuid::nil(),
    }
}
