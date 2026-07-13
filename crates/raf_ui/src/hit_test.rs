use serde::{Deserialize, Serialize};

use crate::geometry::UiRect;
use crate::node::UiNodeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiHitTestMode {
    InteractiveOnly,
    Any,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiHitRegion {
    pub id: String,
    pub kind: UiNodeKind,
    pub rect: UiRect,
    pub z_index: i16,
    pub interactive: bool,
    pub focusable: bool,
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiHitResult {
    pub id: String,
    pub kind: UiNodeKind,
    pub rect: UiRect,
    pub z_index: i16,
    pub focusable: bool,
}

pub fn hit_test(
    regions: &[UiHitRegion],
    point: [f32; 2],
    mode: UiHitTestMode,
) -> Option<UiHitResult> {
    let mut best: Option<(usize, &UiHitRegion)> = None;

    for (index, region) in regions.iter().enumerate() {
        if region.disabled || !region.rect.contains(point) {
            continue;
        }
        if mode == UiHitTestMode::InteractiveOnly && !region.interactive {
            continue;
        }

        let replace = match best {
            None => true,
            Some((best_index, best_region)) => {
                region.z_index > best_region.z_index
                    || (region.z_index == best_region.z_index && index > best_index)
            }
        };

        if replace {
            best = Some((index, region));
        }
    }

    best.map(|(_, region)| UiHitResult {
        id: region.id.clone(),
        kind: region.kind,
        rect: region.rect,
        z_index: region.z_index,
        focusable: region.focusable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test_prefers_higher_z_index() {
        let regions = vec![
            UiHitRegion {
                id: "base".to_string(),
                kind: UiNodeKind::Panel,
                rect: UiRect::new(0.0, 0.0, 100.0, 100.0),
                z_index: 0,
                interactive: true,
                focusable: false,
                disabled: false,
            },
            UiHitRegion {
                id: "menu".to_string(),
                kind: UiNodeKind::Menu,
                rect: UiRect::new(10.0, 10.0, 80.0, 80.0),
                z_index: 10,
                interactive: true,
                focusable: true,
                disabled: false,
            },
        ];

        let hit = hit_test(&regions, [20.0, 20.0], UiHitTestMode::InteractiveOnly).unwrap();
        assert_eq!(hit.id, "menu");
        assert!(hit.focusable);
    }

    #[test]
    fn disabled_regions_do_not_hit() {
        let regions = vec![UiHitRegion {
            id: "button".to_string(),
            kind: UiNodeKind::Button,
            rect: UiRect::new(0.0, 0.0, 100.0, 40.0),
            z_index: 0,
            interactive: true,
            focusable: true,
            disabled: true,
        }];

        assert!(hit_test(&regions, [10.0, 10.0], UiHitTestMode::InteractiveOnly).is_none());
    }
}
