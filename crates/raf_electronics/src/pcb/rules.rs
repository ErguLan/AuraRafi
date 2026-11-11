//! Portable two-layer design rules, shared by authoring and manufacturing.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NetRules {
    pub clearance: f32,
    pub min_width: f32,
}

impl Default for NetRules {
    fn default() -> Self {
        Self {
            clearance: 0.2,
            min_width: 0.1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PcbDesignRules {
    pub default: NetRules,
    pub edge_clearance: f32,
    pub min_drill: f32,
    pub min_annular_ring: f32,
    pub via_diameter: f32,
    pub via_drill: f32,
    /// An absent override inherits the default. Keys are electrical net names.
    pub nets: BTreeMap<String, NetRules>,
}

impl Default for PcbDesignRules {
    fn default() -> Self {
        Self {
            default: NetRules::default(),
            edge_clearance: 0.2,
            min_drill: 0.2,
            min_annular_ring: 0.15,
            via_diameter: 0.7,
            via_drill: 0.3,
            nets: BTreeMap::new(),
        }
    }
}

impl PcbDesignRules {
    pub fn for_net(&self, net: &str) -> NetRules {
        self.nets.get(net).copied().unwrap_or(self.default)
    }
    pub fn clearance(&self, a: &str, b: &str) -> f32 {
        self.for_net(a).clearance.max(self.for_net(b).clearance)
    }
    pub fn max_clearance(&self) -> f32 {
        self.nets
            .values()
            .fold(self.default.clearance, |gap, r| gap.max(r.clearance))
    }
    pub fn is_valid(&self) -> bool {
        let positive = |n: f32| n.is_finite() && n > 0.0 && n <= 10.0;
        let valid_net = |r: NetRules| positive(r.clearance) && positive(r.min_width);
        valid_net(self.default)
            && self
                .nets
                .iter()
                .all(|(net, r)| !net.trim().is_empty() && valid_net(*r))
            && [
                self.edge_clearance,
                self.min_drill,
                self.min_annular_ring,
                self.via_diameter,
                self.via_drill,
            ]
            .into_iter()
            .all(positive)
            && self.via_drill >= self.min_drill
            && self.via_diameter - self.via_drill + 0.0001 >= 2.0 * self.min_annular_ring
    }
}
