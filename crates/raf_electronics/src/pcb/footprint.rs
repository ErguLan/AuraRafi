use glam::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FootprintPadDefinition {
    pub name: String,
    pub offset: Vec2,
    pub size: Vec2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FootprintDefinition {
    pub id: String,
    pub body_size: Vec2,
    pub pads: Vec<FootprintPadDefinition>,
    pub preview_asset: Option<String>,
}

pub fn footprint_definition(id: &str, pin_count: usize) -> FootprintDefinition {
    match id {
        "0805" => FootprintDefinition {
            id: id.to_string(),
            body_size: Vec2::new(36.0, 18.0),
            pads: vec![
                FootprintPadDefinition {
                    name: "1".to_string(),
                    offset: Vec2::new(-18.0, 0.0),
                    size: Vec2::new(12.0, 10.0),
                },
                FootprintPadDefinition {
                    name: "2".to_string(),
                    offset: Vec2::new(18.0, 0.0),
                    size: Vec2::new(12.0, 10.0),
                },
            ],
            preview_asset: Some("footprints/0805.png".to_string()),
        },
        "MAG-10x5" => FootprintDefinition {
            id: id.to_string(),
            body_size: Vec2::new(72.0, 36.0),
            pads: vec![
                FootprintPadDefinition {
                    name: "1".to_string(),
                    offset: Vec2::new(-26.0, 0.0),
                    size: Vec2::new(12.0, 12.0),
                },
                FootprintPadDefinition {
                    name: "2".to_string(),
                    offset: Vec2::new(26.0, 0.0),
                    size: Vec2::new(12.0, 12.0),
                },
            ],
            preview_asset: Some("footprints/magnet-10x5.png".to_string()),
        },
        "BAT-18650" => FootprintDefinition {
            id: id.to_string(),
            body_size: Vec2::new(120.0, 42.0),
            pads: vec![
                FootprintPadDefinition {
                    name: "+".to_string(),
                    offset: Vec2::new(-48.0, 0.0),
                    size: Vec2::new(16.0, 14.0),
                },
                FootprintPadDefinition {
                    name: "-".to_string(),
                    offset: Vec2::new(48.0, 0.0),
                    size: Vec2::new(16.0, 14.0),
                },
            ],
            preview_asset: Some("footprints/battery-18650.png".to_string()),
        },
        "TP-GND" => FootprintDefinition {
            id: id.to_string(),
            body_size: Vec2::new(28.0, 28.0),
            pads: vec![
                FootprintPadDefinition {
                    name: "TP".to_string(),
                    offset: Vec2::ZERO,
                    size: Vec2::new(16.0, 16.0),
                },
            ],
            preview_asset: Some("footprints/test-point.png".to_string()),
        },
        _ => generic_footprint(id, pin_count.max(1)),
    }
}

fn generic_footprint(id: &str, pin_count: usize) -> FootprintDefinition {
    let pads = if pin_count <= 2 {
        vec![
            FootprintPadDefinition {
                name: "1".to_string(),
                offset: Vec2::new(-20.0, 0.0),
                size: Vec2::new(12.0, 10.0),
            },
            FootprintPadDefinition {
                name: "2".to_string(),
                offset: Vec2::new(20.0, 0.0),
                size: Vec2::new(12.0, 10.0),
            },
        ]
    } else {
        let left_count = ((pin_count as f32) / 2.0).ceil() as usize;
        let right_count = pin_count.saturating_sub(left_count);
        let row_pitch = 14.0;
        let left_span = (left_count.saturating_sub(1)) as f32 * row_pitch;
        let right_span = (right_count.saturating_sub(1)) as f32 * row_pitch;
        let mut pads = Vec::with_capacity(pin_count);

        for idx in 0..left_count {
            pads.push(FootprintPadDefinition {
                name: (idx + 1).to_string(),
                offset: Vec2::new(-28.0, idx as f32 * row_pitch - left_span * 0.5),
                size: Vec2::new(10.0, 8.0),
            });
        }

        for idx in 0..right_count {
            pads.push(FootprintPadDefinition {
                name: (left_count + idx + 1).to_string(),
                offset: Vec2::new(28.0, idx as f32 * row_pitch - right_span * 0.5),
                size: Vec2::new(10.0, 8.0),
            });
        }

        pads
    };

    FootprintDefinition {
        id: id.to_string(),
        body_size: Vec2::new(52.0, (pin_count.max(2) as f32 * 8.0).max(24.0)),
        pads,
        preview_asset: None,
    }
}